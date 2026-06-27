//! Materialises `OpenFGA` relationship tuples from the seeded Postgres org graph.
//!
//! `infra/seed/demo-seed.sql` writes rows straight into Postgres, bypassing the
//! services that would emit the matching authz tuples; this one-shot tool reads the
//! org graph back and re-issues the grants through [`application::permissions::Permissions`].
//! Only Postgres + `OpenFGA` are touched, so it runs without a full server boot. Run
//! via `cargo make seed` (after the SQL). `OpenFGA` rejects re-writing an existing
//! tuple, so a re-run logs per-tuple warnings and is otherwise a no-op.

use std::{collections::HashSet, sync::Arc};

use anyhow::{Context, Result};
use application::permissions::Permissions;
use domain::{
    ids::TicketId,
    ports::authz_client::AuthzClient,
    repository::{GroupRepository, ProjectRepository, TicketRepository, UserRepository},
};
use infrastructure::{
    openfga::{self, OpenFgaAuthzClient},
    postgres::{PgGroupRepo, PgProjectRepo, PgTicketRepo, PgUserRepo, build_pool},
};

/// Tallies successful vs. rejected tuple writes (rejections are mostly
/// "already exists" on a re-run).
#[derive(Default)]
struct Tally {
    ok: usize,
    failed: usize,
}

impl Tally {
    fn record(&mut self, label: &str, result: application::Result<()>) {
        match result {
            Ok(()) => self.ok += 1,
            Err(e) => {
                self.failed += 1;
                eprintln!("  warn: {label}: {e}");
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").context("missing DATABASE_URL")?;
    let openfga_api_url = std::env::var("OPENFGA_API_URL").context("missing OPENFGA_API_URL")?;
    let model_path = std::env::var("OPENFGA_MODEL_PATH")
        .unwrap_or_else(|_| "infra/openfga/authorization-model.json".to_owned());
    let bearer_token = std::env::var("OPENFGA_BEARER_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());

    let pool = build_pool(&database_url, 5)
        .await
        .context("building postgres pool")?;

    let users: Arc<dyn UserRepository> = Arc::new(PgUserRepo::new(pool.clone()));
    let groups: Arc<dyn GroupRepository> = Arc::new(PgGroupRepo::new(pool.clone()));
    let projects: Arc<dyn ProjectRepository> = Arc::new(PgProjectRepo::new(pool.clone()));
    let tickets: Arc<dyn TicketRepository> = Arc::new(PgTicketRepo::new(pool.clone()));

    // Resolve (get-or-create) the OpenFGA store + authorization model, exactly as
    // the server does at startup, so this runs against the same store.
    let model_json = tokio::fs::read_to_string(&model_path)
        .await
        .with_context(|| format!("reading openfga model from {model_path}"))?;
    let fga_config = openfga::resolve_config(&openfga_api_url, "portal", &model_json, bearer_token)
        .await
        .context("resolving openfga store/model")?;
    let authz: Arc<dyn AuthzClient> =
        Arc::new(OpenFgaAuthzClient::new(fga_config).context("building openfga client")?);

    let perms = Permissions::new(users.clone(), groups.clone(), authz);

    let mut tally = Tally::default();

    // 1. Company-wide member wildcard (also seeded by the server at boot; idempotent).
    tally.record(
        "company#member=user:*",
        perms.seed_company_member_wildcard().await,
    );

    // 2. Org-wide system roles (director / hr) -> company#director / company#hr.
    let active_users = load_active_users(users.as_ref()).await?;
    for user in &active_users {
        if let Some(role) = user.system_role {
            tally.record(
                &format!("company_role {}", user.email),
                perms.grant_company_role(user.id, role).await,
            );
        }
    }

    // 3. Groups -> group#company, and each active membership -> group#<role>.
    let all_groups = groups.list_all().await.context("listing groups")?;
    for group in &all_groups {
        tally.record(
            &format!("group_created {}", group.name),
            perms.grant_group_created(group.id).await,
        );
        let memberships = groups
            .list_memberships_for_group(group.id)
            .await
            .with_context(|| format!("listing memberships for {}", group.name))?;
        for membership in &memberships {
            if membership.is_active() {
                tally.record(
                    "group_membership",
                    perms.grant_group_membership(membership).await,
                );
            }
        }

        // 4. Projects owned by this group -> project#owner_group + project#company,
        //    plus each collaborator group -> project#collaborator_group.
        let owned = projects
            .list_for_owner_group(group.id, None)
            .await
            .with_context(|| format!("listing projects for {}", group.name))?;
        for project in &owned {
            tally.record(
                &format!("project_created {}", project.name),
                perms
                    .grant_project_created(project.owner_group_id, project.id)
                    .await,
            );
            let collaborators = projects
                .list_collaborators(project.id)
                .await
                .with_context(|| format!("listing collaborators for {}", project.name))?;
            for collab in &collaborators {
                tally.record(
                    "project_collaborator",
                    perms
                        .grant_project_collaborator(collab.group_id, collab.project_id)
                        .await,
                );
            }
        }
    }

    // 5. Tickets. No list-all exists, so union the per-requester lists over active
    //    users (every ticket has a requester); dedupe in case of overlap.
    let mut seen: HashSet<TicketId> = HashSet::new();
    for user in &active_users {
        let raised = tickets
            .list_for_requester(user.id, None)
            .await
            .with_context(|| format!("listing tickets for {}", user.email))?;
        for ticket in raised {
            if !seen.insert(ticket.id) {
                continue;
            }
            tally.record(
                &format!("ticket_created {}", ticket.id.0),
                perms
                    .grant_ticket_created(ticket.requester_user_id, ticket.id)
                    .await,
            );
            if let Some(assignee) = ticket.assignee_user_id {
                tally.record(
                    "ticket_assignee",
                    perms.grant_ticket_assignee(assignee, ticket.id).await,
                );
            }
        }
    }

    println!(
        "seed_authz: {} tuples written, {} rejected (rejections are expected on a re-run).",
        tally.ok, tally.failed
    );
    Ok(())
}

/// Pages through every active user (`list_active` is the only enumeration the
/// repository exposes).
async fn load_active_users(users: &dyn UserRepository) -> Result<Vec<domain::model::User>> {
    const PAGE: u32 = 500;
    let mut all = Vec::new();
    let mut offset = 0u32;
    loop {
        let page = users
            .list_active(PAGE, offset, None)
            .await
            .context("listing active users")?;
        let len = u32::try_from(page.len()).unwrap_or(u32::MAX);
        all.extend(page);
        if len < PAGE {
            break;
        }
        offset += len;
    }
    Ok(all)
}

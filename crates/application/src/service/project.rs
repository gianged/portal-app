use std::sync::Arc;

use domain::{
    ids::{GroupId, ProjectCollaboratorId, ProjectId, ProjectInviteId, UserId},
    model::{
        Project, ProjectCollaborator, ProjectInvite, ProjectInviteStatus, ProjectStatus,
        RequestStatus,
    },
    repository::{ProjectRepository, RequestRepository},
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    commands::project::{CreateProjectCommand, UpdateProjectMetadataCommand},
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

const OPEN_REQUEST_STATUSES: &[RequestStatus] = &[
    RequestStatus::Draft,
    RequestStatus::Submitted,
    RequestStatus::Assigned,
    RequestStatus::InProgress,
    RequestStatus::Review,
];

pub struct ProjectService {
    projects: Arc<dyn ProjectRepository>,
    requests: Arc<dyn RequestRepository>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl ProjectService {
    #[must_use]
    pub fn new(
        projects: Arc<dyn ProjectRepository>,
        requests: Arc<dyn RequestRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            projects,
            requests,
            perms,
            events,
        }
    }

    /// Creates a new project owned by the given group, in `Planning` status.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not a leader of the owner group, or a repository, event, or authz-backed repository error if the datastore or event bus is unavailable.
    pub async fn create_project(
        &self,
        actor: UserId,
        cmd: CreateProjectCommand,
    ) -> Result<Project> {
        self.perms
            .require_group_leader(actor, cmd.owner_group_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        let project = Project {
            id: ProjectId(Uuid::now_v7()),
            owner_group_id: cmd.owner_group_id,
            created_by_user_id: actor,
            name: cmd.name,
            description: cmd.description,
            status: ProjectStatus::Planning,
            created_at: now,
            updated_at: now,
        };
        self.projects.save_project(&project).await?;
        // OpenFGA: bind the project to its owner group (drives owner_member /
        // viewer) and the company singleton (Director viewer branch).
        self.perms
            .grant_project_created(project.owner_group_id, project.id)
            .await?;
        self.events
            .emit(DomainEvent::ProjectCreated {
                project_id: project.id,
                owner_group: project.owner_group_id,
                actor,
                at: now,
                after: project.clone(),
            })
            .await?;
        Ok(project)
    }

    /// Updates the project's name and/or description.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist, `Forbidden` if the actor is not a leader or sub-leader of the owner group, or a repository or event error if the datastore or event bus is unavailable.
    pub async fn update_metadata(
        &self,
        actor: UserId,
        project_id: ProjectId,
        cmd: UpdateProjectMetadataCommand,
    ) -> Result<Project> {
        let mut project = self.load(project_id).await?;
        self.perms
            .require_group_leader_or_sub(actor, project.owner_group_id)
            .await?;
        let before = project.clone();
        let now = OffsetDateTime::now_utc();
        if let Some(name) = cmd.name {
            project.name = name;
        }
        if let Some(description) = cmd.description {
            project.description = description;
        }
        project.updated_at = now;
        self.projects.save_project(&project).await?;
        self.events
            .emit(DomainEvent::ProjectMetadataUpdated {
                project_id: project.id,
                actor,
                at: now,
                before,
                after: project.clone(),
            })
            .await?;
        Ok(project)
    }

    /// Transitions the project to `Active`.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist, `Forbidden` if the actor is not a leader of the owner group, `Transition` if the project is not in an activatable state, or a repository or event error if the datastore or event bus is unavailable.
    pub async fn activate(&self, actor: UserId, project_id: ProjectId) -> Result<Project> {
        self.transition(actor, project_id, Project::activate).await
    }

    /// Transitions the project to `OnHold`.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist, `Forbidden` if the actor is not a leader of the owner group, `Transition` if the project cannot be put on hold from its current state, or a repository or event error if the datastore or event bus is unavailable.
    pub async fn hold(&self, actor: UserId, project_id: ProjectId) -> Result<Project> {
        self.transition(actor, project_id, Project::hold).await
    }

    /// Resumes an on-hold project back to `Active`.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist, `Forbidden` if the actor is not a leader of the owner group, `Transition` if the project is not on hold, or a repository or event error if the datastore or event bus is unavailable.
    pub async fn resume(&self, actor: UserId, project_id: ProjectId) -> Result<Project> {
        self.transition(actor, project_id, Project::resume).await
    }

    /// Transitions the project to `Completed` and cascade-cancels its open requests.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist, `Forbidden` if the actor is not a leader of the owner group, `Transition` if the project cannot be completed from its current state (or an open request cannot be cancelled), or a repository or event error if the datastore or event bus is unavailable.
    pub async fn complete(&self, actor: UserId, project_id: ProjectId) -> Result<Project> {
        let project = self
            .transition(actor, project_id, Project::complete)
            .await?;
        self.cascade_cancel_open_requests(actor, project_id).await?;
        Ok(project)
    }

    /// Transitions the project to `Cancelled` and cascade-cancels its open requests.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist, `Forbidden` if the actor is not a leader of the owner group, `Transition` if the project cannot be cancelled from its current state (or an open request cannot be cancelled), or a repository or event error if the datastore or event bus is unavailable.
    pub async fn cancel(&self, actor: UserId, project_id: ProjectId) -> Result<Project> {
        let project = self.transition(actor, project_id, Project::cancel).await?;
        self.cascade_cancel_open_requests(actor, project_id).await?;
        Ok(project)
    }

    /// Invites a group to collaborate on the project, creating a pending invite.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist, `Forbidden` if the actor is not a leader of the owner group, `Validation` if the target group is the owner group, `Conflict` if the project is not active or the group is already a collaborator or already has a pending invite, or a repository or event error if the datastore or event bus is unavailable.
    pub async fn invite_group(
        &self,
        actor: UserId,
        project_id: ProjectId,
        target_group: GroupId,
    ) -> Result<ProjectInvite> {
        let project = self.load(project_id).await?;
        self.perms
            .require_group_leader(actor, project.owner_group_id)
            .await?;
        if project.status != ProjectStatus::Active {
            return Err(Error::Conflict("project_not_active".into()));
        }
        if target_group == project.owner_group_id {
            return Err(Error::Validation("owner_cannot_collaborate_on_self".into()));
        }

        let collaborators = self.projects.list_collaborators(project_id).await?;
        if collaborators.iter().any(|c| c.group_id == target_group) {
            return Err(Error::Conflict("group_already_collaborator".into()));
        }

        let pending = self
            .projects
            .list_pending_invites_for_group(target_group)
            .await?;
        if pending.iter().any(|i| i.project_id == project_id) {
            return Err(Error::Conflict("invite_already_pending".into()));
        }

        let now = OffsetDateTime::now_utc();
        let invite = ProjectInvite {
            id: ProjectInviteId(Uuid::now_v7()),
            project_id,
            invited_by_user_id: actor,
            invited_group_id: target_group,
            responded_by_user_id: None,
            status: ProjectInviteStatus::Pending,
            responded_at: None,
            created_at: now,
            updated_at: now,
        };
        self.projects.save_invite(&invite).await?;
        self.events
            .emit(DomainEvent::ProjectInviteSent {
                invite_id: invite.id,
                project_id,
                target_group,
                actor,
                at: now,
            })
            .await?;
        Ok(invite)
    }

    /// Accepts a pending invite, adding the invited group as a collaborator.
    ///
    /// # Errors
    /// Returns `NotFound` if the invite does not exist, `Forbidden` if the actor is not a leader of the invited group, `Transition` if the invite is not pending, or a repository, event, or authz-backed repository error if the datastore or event bus is unavailable.
    pub async fn accept_invite(
        &self,
        actor: UserId,
        invite_id: ProjectInviteId,
    ) -> Result<ProjectInvite> {
        let mut invite = self
            .projects
            .find_invite(invite_id)
            .await?
            .ok_or(Error::NotFound("invite"))?;
        self.perms
            .require_group_leader(actor, invite.invited_group_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        invite.accept(actor, now)?;

        let collaborator = ProjectCollaborator {
            id: ProjectCollaboratorId(Uuid::now_v7()),
            project_id: invite.project_id,
            group_id: invite.invited_group_id,
            created_at: now,
            updated_at: now,
        };
        self.projects.save_collaborator(&collaborator).await?;
        self.projects.save_invite(&invite).await?;
        // OpenFGA: the invited group's members become collaborator_member ->
        // viewer on the project. Subsequent membership changes propagate
        // automatically via the group's member tuples.
        self.perms
            .grant_project_collaborator(invite.invited_group_id, invite.project_id)
            .await?;
        self.events
            .emit(DomainEvent::ProjectInviteResponded {
                invite_id: invite.id,
                project_id: invite.project_id,
                target_group: invite.invited_group_id,
                status: invite.status,
                actor,
                at: now,
            })
            .await?;
        Ok(invite)
    }

    /// Declines a pending invite on behalf of the invited group.
    ///
    /// # Errors
    /// Returns `NotFound` if the invite does not exist, `Forbidden` if the actor is not a leader of the invited group, `Transition` if the invite is not pending, or a repository or event error if the datastore or event bus is unavailable.
    pub async fn decline_invite(
        &self,
        actor: UserId,
        invite_id: ProjectInviteId,
    ) -> Result<ProjectInvite> {
        let mut invite = self
            .projects
            .find_invite(invite_id)
            .await?
            .ok_or(Error::NotFound("invite"))?;
        self.perms
            .require_group_leader(actor, invite.invited_group_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        invite.decline(actor, now)?;
        self.projects.save_invite(&invite).await?;
        self.events
            .emit(DomainEvent::ProjectInviteResponded {
                invite_id: invite.id,
                project_id: invite.project_id,
                target_group: invite.invited_group_id,
                status: invite.status,
                actor,
                at: now,
            })
            .await?;
        Ok(invite)
    }

    /// Revokes a pending invite on behalf of the project's owner group.
    ///
    /// # Errors
    /// Returns `NotFound` if the invite or its project does not exist, `Forbidden` if the actor is not a leader of the owner group, `Transition` if the invite is not pending, or a repository or event error if the datastore or event bus is unavailable.
    pub async fn revoke_invite(
        &self,
        actor: UserId,
        invite_id: ProjectInviteId,
    ) -> Result<ProjectInvite> {
        let mut invite = self
            .projects
            .find_invite(invite_id)
            .await?
            .ok_or(Error::NotFound("invite"))?;
        let project = self.load(invite.project_id).await?;
        self.perms
            .require_group_leader(actor, project.owner_group_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        invite.revoke(now)?;
        self.projects.save_invite(&invite).await?;
        self.events
            .emit(DomainEvent::ProjectInviteResponded {
                invite_id: invite.id,
                project_id: invite.project_id,
                target_group: invite.invited_group_id,
                status: invite.status,
                actor,
                at: now,
            })
            .await?;
        Ok(invite)
    }

    /// Removes a collaborator group from the project.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist or the group is not a collaborator, `Forbidden` if the actor is not a leader of the owner group, or a repository, event, or authz-backed repository error if the datastore or event bus is unavailable.
    pub async fn remove_collaborator(
        &self,
        actor: UserId,
        project_id: ProjectId,
        group_id: GroupId,
    ) -> Result<()> {
        let project = self.load(project_id).await?;
        self.perms
            .require_group_leader(actor, project.owner_group_id)
            .await?;
        let collaborators = self.projects.list_collaborators(project_id).await?;
        let collaborator = collaborators
            .into_iter()
            .find(|c| c.group_id == group_id)
            .ok_or(Error::NotFound("collaborator"))?;
        self.projects.delete_collaborator(collaborator.id).await?;
        self.perms
            .revoke_project_collaborator(group_id, project_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        self.events
            .emit(DomainEvent::ProjectCollaboratorRemoved {
                project_id,
                group_id,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    /// Finds a project the actor is permitted to view.
    ///
    /// # Errors
    /// Returns `NotFound` if the project does not exist, `Forbidden` if the actor cannot view it, or a repository or authz-backed repository error if the datastore is unavailable.
    pub async fn find(&self, actor: UserId, id: ProjectId) -> Result<Project> {
        let project = self.load(id).await?;
        self.perms.require_can_view_project(actor, id).await?;
        Ok(project)
    }

    /// Lists the project's collaborator groups.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot view the project, or a repository or authz-backed repository error if the datastore is unavailable.
    pub async fn list_collaborators(
        &self,
        actor: UserId,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectCollaborator>> {
        self.perms
            .require_can_view_project(actor, project_id)
            .await?;
        Ok(self.projects.list_collaborators(project_id).await?)
    }

    /// Lists projects owned by the given group.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not a member of the group, or a repository or authz-backed repository error if the datastore is unavailable.
    pub async fn list_for_owner_group(
        &self,
        actor: UserId,
        group_id: GroupId,
        q: Option<&str>,
    ) -> Result<Vec<Project>> {
        self.perms.require_group_member(actor, group_id).await?;
        Ok(self.projects.list_for_owner_group(group_id, q).await?)
    }

    /// Lists pending invites addressed to the given group.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not a leader or sub-leader of the group, or a repository or authz-backed repository error if the datastore is unavailable.
    pub async fn list_pending_invites_for_group(
        &self,
        actor: UserId,
        group_id: GroupId,
    ) -> Result<Vec<ProjectInvite>> {
        self.perms
            .require_group_leader_or_sub(actor, group_id)
            .await?;
        Ok(self
            .projects
            .list_pending_invites_for_group(group_id)
            .await?)
    }

    /// Lists pending invites issued for the given project.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot view the project, or a repository or authz-backed repository error if the datastore is unavailable.
    pub async fn list_pending_invites_for_project(
        &self,
        actor: UserId,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectInvite>> {
        self.perms
            .require_can_view_project(actor, project_id)
            .await?;
        Ok(self
            .projects
            .list_pending_invites_for_project(project_id)
            .await?)
    }

    async fn transition<F>(&self, actor: UserId, project_id: ProjectId, op: F) -> Result<Project>
    where
        F: FnOnce(
            &mut Project,
            OffsetDateTime,
        ) -> std::result::Result<(), domain::error::TransitionError>,
    {
        let mut project = self.load(project_id).await?;
        self.perms
            .require_group_leader(actor, project.owner_group_id)
            .await?;
        let from = project.status;
        let now = OffsetDateTime::now_utc();
        op(&mut project, now)?;
        self.projects.save_project(&project).await?;
        self.events
            .emit(DomainEvent::ProjectStatusChanged {
                project_id: project.id,
                from,
                to: project.status,
                actor,
                at: now,
            })
            .await?;
        Ok(project)
    }

    async fn cascade_cancel_open_requests(
        &self,
        actor: UserId,
        project_id: ProjectId,
    ) -> Result<()> {
        let now = OffsetDateTime::now_utc();
        for status in OPEN_REQUEST_STATUSES {
            let open = self
                .requests
                .list_for_project(project_id, Some(*status), None)
                .await?;
            for mut request in open {
                let from = request.status;
                request.cancel(now)?;
                self.requests.save(&request).await?;
                self.events
                    .emit(DomainEvent::RequestStatusChanged {
                        request_id: request.id,
                        project_id,
                        from,
                        to: request.status,
                        actor,
                        at: now,
                    })
                    .await?;
            }
        }
        Ok(())
    }

    async fn load(&self, project_id: ProjectId) -> Result<Project> {
        self.projects
            .find_by_id(project_id)
            .await?
            .ok_or(Error::NotFound("project"))
    }
}

use std::{collections::HashSet, sync::Arc};

use domain::{
    ids::{ProjectId, RequestAttachmentId, RequestId, UserId},
    model::{Request, RequestAttachment, RequestStatus},
    ports::file_storage::FileStorage,
    repository::{GroupRepository, ProjectRepository, RequestRepository},
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    commands::request::{AddAttachmentCommand, CreateRequestCommand},
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

pub struct RequestService {
    requests: Arc<dyn RequestRepository>,
    projects: Arc<dyn ProjectRepository>,
    groups: Arc<dyn GroupRepository>,
    storage: Arc<dyn FileStorage>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl RequestService {
    #[must_use]
    pub fn new(
        requests: Arc<dyn RequestRepository>,
        projects: Arc<dyn ProjectRepository>,
        groups: Arc<dyn GroupRepository>,
        storage: Arc<dyn FileStorage>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            requests,
            projects,
            groups,
            storage,
            perms,
            events,
        }
    }

    pub async fn create(
        &self,
        actor: UserId,
        cmd: CreateRequestCommand,
    ) -> Result<Request> {
        self.perms.require_active(actor).await?;
        self.perms
            .require_can_view_project(actor, cmd.project_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        let request = Request {
            id: RequestId(Uuid::now_v7()),
            project_id: cmd.project_id,
            creator_user_id: actor,
            assignee_user_id: None,
            title: cmd.title,
            description: cmd.description,
            status: RequestStatus::Draft,
            priority: cmd.priority,
            due_at: cmd.due_at,
            created_at: now,
            updated_at: now,
        };
        self.requests.save(&request).await?;
        self.events
            .emit(DomainEvent::RequestCreated {
                request_id: request.id,
                project_id: request.project_id,
                actor,
                at: now,
                after: request.clone(),
            })
            .await?;
        Ok(request)
    }

    pub async fn submit(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        let mut request = self.load(request_id).await?;
        if request.creator_user_id != actor {
            return Err(Error::Forbidden);
        }
        let from = request.status;
        let now = OffsetDateTime::now_utc();
        request.submit(now)?;
        self.requests.save(&request).await?;
        self.emit_status(actor, &request, from, now).await?;
        Ok(request)
    }

    pub async fn assign(
        &self,
        actor: UserId,
        request_id: RequestId,
        assignee: UserId,
    ) -> Result<Request> {
        let mut request = self.load(request_id).await?;
        self.perms
            .require_can_assign_request(actor, request.project_id)
            .await?;
        self.assert_assignee_eligible(request.project_id, assignee).await?;
        let from = request.status;
        let now = OffsetDateTime::now_utc();
        request.assign(assignee, now)?;
        self.requests.save(&request).await?;
        self.events
            .emit(DomainEvent::RequestAssigned {
                request_id: request.id,
                project_id: request.project_id,
                assignee,
                actor,
                at: now,
            })
            .await?;
        self.emit_status(actor, &request, from, now).await?;
        Ok(request)
    }

    pub async fn start(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        let mut request = self.load(request_id).await?;
        if request.assignee_user_id != Some(actor) {
            return Err(Error::Forbidden);
        }
        let from = request.status;
        let now = OffsetDateTime::now_utc();
        request.start(now)?;
        self.requests.save(&request).await?;
        self.emit_status(actor, &request, from, now).await?;
        Ok(request)
    }

    pub async fn send_for_review(
        &self,
        actor: UserId,
        request_id: RequestId,
    ) -> Result<Request> {
        let mut request = self.load(request_id).await?;
        if request.assignee_user_id != Some(actor) {
            return Err(Error::Forbidden);
        }
        let from = request.status;
        let now = OffsetDateTime::now_utc();
        request.send_for_review(now)?;
        self.requests.save(&request).await?;
        self.emit_status(actor, &request, from, now).await?;
        Ok(request)
    }

    pub async fn approve(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        let mut request = self.load(request_id).await?;
        self.require_approver(actor, &request).await?;
        let from = request.status;
        let now = OffsetDateTime::now_utc();
        request.complete(now)?;
        self.requests.save(&request).await?;
        self.emit_status(actor, &request, from, now).await?;
        Ok(request)
    }

    pub async fn reject(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        let mut request = self.load(request_id).await?;
        self.require_approver(actor, &request).await?;
        let from = request.status;
        let now = OffsetDateTime::now_utc();
        request.reject(now)?;
        self.requests.save(&request).await?;
        self.emit_status(actor, &request, from, now).await?;
        Ok(request)
    }

    pub async fn cancel(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        let mut request = self.load(request_id).await?;
        let is_creator = request.creator_user_id == actor;
        let is_assignee = request.assignee_user_id == Some(actor);
        if !is_creator && !is_assignee {
            self.perms
                .require_can_assign_request(actor, request.project_id)
                .await?;
        }
        let from = request.status;
        let now = OffsetDateTime::now_utc();
        request.cancel(now)?;
        self.requests.save(&request).await?;
        self.emit_status(actor, &request, from, now).await?;
        Ok(request)
    }

    pub async fn add_attachment(
        &self,
        actor: UserId,
        request_id: RequestId,
        cmd: AddAttachmentCommand,
    ) -> Result<RequestAttachment> {
        let request = self.load(request_id).await?;
        self.perms
            .require_can_view_project(actor, request.project_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        let attachment_id = RequestAttachmentId(Uuid::now_v7());
        let storage_key = format!(
            "request-attachments/{}/{}/{}",
            request.id.0, attachment_id.0, cmd.filename
        );
        let size_bytes = u64::try_from(cmd.bytes.len())
            .map_err(|_| Error::Validation("attachment_too_large".into()))?;
        self.storage
            .put(&storage_key, &cmd.content_type, cmd.bytes)
            .await?;
        let attachment = RequestAttachment {
            id: attachment_id,
            request_id,
            uploaded_by_user_id: actor,
            filename: cmd.filename,
            content_type: cmd.content_type,
            size_bytes,
            storage_key,
            created_at: now,
        };
        self.requests.save_attachment(&attachment).await?;
        Ok(attachment)
    }

    pub async fn list_for_project(
        &self,
        actor: UserId,
        project_id: ProjectId,
        status: Option<RequestStatus>,
    ) -> Result<Vec<Request>> {
        self.perms.require_can_view_project(actor, project_id).await?;
        Ok(self.requests.list_for_project(project_id, status).await?)
    }

    pub async fn list_for_assignee(
        &self,
        actor: UserId,
        status: Option<RequestStatus>,
    ) -> Result<Vec<Request>> {
        self.perms.require_active(actor).await?;
        Ok(self.requests.list_for_assignee(actor, status).await?)
    }

    async fn require_approver(&self, actor: UserId, request: &Request) -> Result<()> {
        if request.creator_user_id == actor {
            return Ok(());
        }
        self.perms
            .require_can_assign_request(actor, request.project_id)
            .await
    }

    async fn assert_assignee_eligible(
        &self,
        project_id: ProjectId,
        assignee: UserId,
    ) -> Result<()> {
        if !self.perms.is_user_active(assignee).await? {
            return Err(Error::Conflict("assignee_not_active".into()));
        }
        let project = self
            .projects
            .find_by_id(project_id)
            .await?
            .ok_or(Error::NotFound("project"))?;
        let collaborators = self.projects.list_collaborators(project_id).await?;
        let mut allowed: HashSet<_> = collaborators.into_iter().map(|c| c.group_id).collect();
        allowed.insert(project.owner_group_id);

        let memberships = self.groups.list_active_memberships_for_user(assignee).await?;
        let in_allowed_group = memberships.iter().any(|m| allowed.contains(&m.group_id));
        if !in_allowed_group {
            return Err(Error::Conflict("assignee_not_eligible".into()));
        }
        Ok(())
    }

    async fn emit_status(
        &self,
        actor: UserId,
        request: &Request,
        from: RequestStatus,
        at: OffsetDateTime,
    ) -> Result<()> {
        self.events
            .emit(DomainEvent::RequestStatusChanged {
                request_id: request.id,
                project_id: request.project_id,
                from,
                to: request.status,
                actor,
                at,
            })
            .await
    }

    async fn load(&self, id: RequestId) -> Result<Request> {
        self.requests
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("request"))
    }
}

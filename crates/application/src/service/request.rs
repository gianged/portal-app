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
    commands::request::{AddAttachmentCommand, CreateRequestCommand, UpdateRequestCommand},
    error::{ConflictCode, Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
    resilience,
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

    /// Creates a new request in `Draft` status under the given project.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or cannot view the project, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn create(&self, actor: UserId, cmd: CreateRequestCommand) -> Result<Request> {
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
            progress: 0,
            due_at: cmd.due_at,
            completed_at: None,
            version: 0,
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

    /// Submits a draft request for assignment. Creator-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the creator, `NotFound` if the request does not exist, `Transition` if the request is not in a submittable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn submit(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
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
        })
        .await
    }

    /// Assigns the request to an eligible assignee.
    ///
    /// # Errors
    /// Returns `NotFound` if the request or its project does not exist, `Forbidden` if the actor cannot assign requests on the project, `Conflict` if the assignee is inactive or not a member of the owner or a collaborator group, `Transition` if the request is not in an assignable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id, assignee = ?assignee))]
    pub async fn assign(
        &self,
        actor: UserId,
        request_id: RequestId,
        assignee: UserId,
    ) -> Result<Request> {
        resilience::retry_stale(|| async {
            let mut request = self.load(request_id).await?;
            self.perms
                .require_can_assign_request(actor, request.project_id)
                .await?;
            self.assert_assignee_eligible(request.project_id, assignee)
                .await?;
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
        })
        .await
    }

    /// Starts work on an assigned request. Assignee-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the assignee, `NotFound` if the request does not exist, `Transition` if the request is not in a startable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn start(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
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
        })
        .await
    }

    /// Sends an in-progress request for review. Assignee-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the assignee, `NotFound` if the request does not exist, `Transition` if the request is not in a reviewable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn send_for_review(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
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
        })
        .await
    }

    /// Approves a request under review, completing it.
    ///
    /// # Errors
    /// Returns `NotFound` if the request does not exist, `Forbidden` if the actor is neither the creator nor able to assign requests on the project, `Transition` if the request is not under review, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn approve(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        resilience::retry_stale(|| async {
            let mut request = self.load(request_id).await?;
            self.require_approver(actor, &request).await?;
            let from = request.status;
            let now = OffsetDateTime::now_utc();
            request.complete(now)?;
            self.requests.save(&request).await?;
            self.emit_status(actor, &request, from, now).await?;
            Ok(request)
        })
        .await
    }

    /// Rejects a request under review, sending it back.
    ///
    /// # Errors
    /// Returns `NotFound` if the request does not exist, `Forbidden` if the actor is neither the creator nor able to assign requests on the project, `Transition` if the request is not under review, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn reject(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        resilience::retry_stale(|| async {
            let mut request = self.load(request_id).await?;
            self.require_approver(actor, &request).await?;
            let from = request.status;
            let now = OffsetDateTime::now_utc();
            request.reject(now)?;
            self.requests.save(&request).await?;
            self.emit_status(actor, &request, from, now).await?;
            Ok(request)
        })
        .await
    }

    /// Cancels a request. Allowed for the creator, the assignee, or anyone who can assign requests on the project.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or is none of the creator, assignee, or an assigner on the project, `NotFound` if the request does not exist, `Transition` if the request cannot be cancelled from its current state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn cancel(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
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
        })
        .await
    }

    /// Sets the completion percentage on an in-progress request. Assignee-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, not the assignee, or the request is not in progress, `NotFound` if the request does not exist, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id, progress = progress))]
    pub async fn set_progress(
        &self,
        actor: UserId,
        request_id: RequestId,
        progress: u8,
    ) -> Result<Request> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
            let mut request = self.load(request_id).await?;
            if request.assignee_user_id != Some(actor)
                || request.status != RequestStatus::InProgress
            {
                return Err(Error::Forbidden);
            }
            let now = OffsetDateTime::now_utc();
            request.set_progress(progress, now);
            self.requests.save(&request).await?;
            self.events
                .emit(DomainEvent::RequestProgressUpdated {
                    request_id: request.id,
                    project_id: request.project_id,
                    actor,
                    at: now,
                })
                .await?;
            Ok(request)
        })
        .await
    }

    /// Uploads an attachment to the request and records its metadata.
    ///
    /// # Errors
    /// Returns `NotFound` if the request does not exist, `Forbidden` if the actor cannot view the project, `Validation` if the attachment size exceeds the representable limit, `Storage` if writing the file fails, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
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

    /// Lists requests under a project, optionally filtered by status.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot view the project, or a repository or authz-backed repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, project_id = ?project_id))]
    pub async fn list_for_project(
        &self,
        actor: UserId,
        project_id: ProjectId,
        status: Option<RequestStatus>,
        q: Option<&str>,
    ) -> Result<Vec<Request>> {
        self.perms
            .require_can_view_project(actor, project_id)
            .await?;
        Ok(self
            .requests
            .list_for_project(project_id, status, q)
            .await?)
    }

    /// Lists requests assigned to the actor, optionally filtered by status.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository or authz-backed repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_for_assignee(
        &self,
        actor: UserId,
        status: Option<RequestStatus>,
        q: Option<&str>,
    ) -> Result<Vec<Request>> {
        self.perms.require_active(actor).await?;
        Ok(self.requests.list_for_assignee(actor, status, q).await?)
    }

    /// Single request, gated by project-view access (same gate as listing).
    ///
    /// # Errors
    /// Returns `NotFound` if the request does not exist, `Forbidden` if the actor cannot view the project, or a repository or authz-backed repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn find(&self, actor: UserId, request_id: RequestId) -> Result<Request> {
        let request = self.load(request_id).await?;
        self.perms
            .require_can_view_project(actor, request.project_id)
            .await?;
        Ok(request)
    }

    /// Lists the attachments on a request.
    ///
    /// # Errors
    /// Returns `NotFound` if the request does not exist, `Forbidden` if the actor cannot view the project, or a repository or authz-backed repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn list_attachments(
        &self,
        actor: UserId,
        request_id: RequestId,
    ) -> Result<Vec<RequestAttachment>> {
        let request = self.load(request_id).await?;
        self.perms
            .require_can_view_project(actor, request.project_id)
            .await?;
        Ok(self.requests.list_attachments(request_id).await?)
    }

    /// Edit request metadata. Creator-only, and only before work starts
    /// (`Draft`/`Submitted`); once assigned the request is frozen to edits here.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the creator, `NotFound` if the request does not exist, `Conflict` if the request is no longer editable (past `Draft`/`Submitted`), or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, request_id = ?request_id))]
    pub async fn update_metadata(
        &self,
        actor: UserId,
        request_id: RequestId,
        cmd: UpdateRequestCommand,
    ) -> Result<Request> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
            let mut request = self.load(request_id).await?;
            if request.creator_user_id != actor {
                return Err(Error::Forbidden);
            }
            if !matches!(
                request.status,
                RequestStatus::Draft | RequestStatus::Submitted
            ) {
                return Err(Error::Conflict(ConflictCode::RequestNotEditable));
            }
            let before = request.clone();
            let now = OffsetDateTime::now_utc();
            if let Some(title) = cmd.title.clone() {
                request.title = title;
            }
            if let Some(description) = cmd.description.clone() {
                request.description = description;
            }
            if let Some(priority) = cmd.priority {
                request.priority = priority;
            }
            if let Some(due_at) = cmd.due_at {
                request.due_at = Some(due_at);
            }
            request.updated_at = now;
            self.requests.save(&request).await?;
            self.events
                .emit(DomainEvent::RequestMetadataUpdated {
                    request_id: request.id,
                    project_id: request.project_id,
                    actor,
                    at: now,
                    before,
                    after: request.clone(),
                })
                .await?;
            Ok(request)
        })
        .await
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
            return Err(Error::Conflict(ConflictCode::AssigneeNotActive));
        }
        let project = self
            .projects
            .find_by_id(project_id)
            .await?
            .ok_or(Error::NotFound("project"))?;
        let collaborators = self.projects.list_collaborators(project_id).await?;
        let mut allowed: HashSet<_> = collaborators.into_iter().map(|c| c.group_id).collect();
        allowed.insert(project.owner_group_id);

        let memberships = self
            .groups
            .list_active_memberships_for_user(assignee)
            .await?;
        let in_allowed_group = memberships.iter().any(|m| allowed.contains(&m.group_id));
        if !in_allowed_group {
            return Err(Error::Conflict(ConflictCode::AssigneeNotEligible));
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

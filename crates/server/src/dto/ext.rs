//! Domain -> wire projections for the external read-only API.

use domain::model;

use shared::dto::ext::{ExtProjectDto, ExtRequestDto};

use super::{project, request};

#[must_use]
pub fn ext_project_dto(p: &model::Project) -> ExtProjectDto {
    ExtProjectDto {
        id: super::project_id(p.id),
        owner_group_id: super::group_id(p.owner_group_id),
        created_by_user_id: super::user_id(p.created_by_user_id),
        name: p.name.clone(),
        description: p.description.clone(),
        status: project::project_status_dto(p.status),
        progress: p.progress,
        completed_at: p.completed_at,
        created_at: p.created_at,
        updated_at: p.updated_at,
    }
}

#[must_use]
pub fn ext_request_dto(r: &model::Request) -> ExtRequestDto {
    ExtRequestDto {
        id: super::request_id(r.id),
        project_id: super::project_id(r.project_id),
        creator_user_id: super::user_id(r.creator_user_id),
        assignee_user_id: r.assignee_user_id.map(super::user_id),
        title: r.title.clone(),
        description: r.description.clone(),
        status: request::request_status_dto(r.status),
        priority: request::request_priority_dto(r.priority),
        progress: r.progress,
        due_at: r.due_at,
        completed_at: r.completed_at,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

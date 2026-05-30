use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    common::UserSummaryDto,
    ids::{TicketId, UserId},
};

/// Mirrors `domain::model::TicketStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Open,
    Triaged,
    Assigned,
    InProgress,
    Resolved,
    Closed,
    Reopened,
}

impl TicketStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Triaged => "Triaged",
            Self::Assigned => "Assigned",
            Self::InProgress => "In Progress",
            Self::Resolved => "Resolved",
            Self::Closed => "Closed",
            Self::Reopened => "Reopened",
        }
    }
}

/// Mirrors `domain::model::TicketPriority`. Set during triage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl TicketPriority {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
            Self::Urgent => "Urgent",
        }
    }
}

/// Mirrors `domain::model::TicketCategory`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketCategory {
    Hardware,
    Software,
    Access,
    Other,
}

impl TicketCategory {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Hardware => "Hardware",
            Self::Software => "Software",
            Self::Access => "Access",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDto {
    pub id: TicketId,
    pub requester: UserSummaryDto,
    pub assignee: Option<UserSummaryDto>,
    pub title: String,
    pub description: String,
    pub status: TicketStatus,
    /// `None` until the ticket is triaged.
    pub priority: Option<TicketPriority>,
    pub category: TicketCategory,
    #[serde(with = "time::serde::rfc3339::option")]
    pub triaged_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub resolved_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub closed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// Maps to `application::commands::RaiseTicketCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaiseTicketRequest {
    pub title: String,
    pub description: String,
    pub category: TicketCategory,
}

/// Triage sets the priority (required once a ticket leaves `open`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageTicketRequest {
    pub priority: TicketPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignTicketRequest {
    pub assignee_user_id: UserId,
}

use domain::{
    ids::{GroupId, UserId},
    model::{GroupKind, GroupRole},
};

/// Input to create a group.
#[derive(Debug, Clone)]
pub struct CreateGroupCommand {
    pub name: String,
    pub description: String,
    pub kind: GroupKind,
}

/// Input to add a user to a group with a role.
#[derive(Debug, Clone)]
pub struct AddMembershipCommand {
    pub group_id: GroupId,
    pub user_id: UserId,
    pub role: GroupRole,
}

/// `None` leaves the field unchanged.
#[derive(Debug, Clone, Default)]
pub struct UpdateGroupMetadataCommand {
    pub name: Option<String>,
    pub description: Option<String>,
}

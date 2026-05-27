use domain::{
    ids::{GroupId, UserId},
    model::{GroupKind, GroupRole},
};

#[derive(Debug, Clone)]
pub struct CreateGroupCommand {
    pub name: String,
    pub description: String,
    pub kind: GroupKind,
}

#[derive(Debug, Clone)]
pub struct AddMembershipCommand {
    pub group_id: GroupId,
    pub user_id: UserId,
    pub role: GroupRole,
}

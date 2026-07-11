use domain::ids::GroupId;

/// Input to create a project under an owner group.
#[derive(Debug, Clone)]
pub struct CreateProjectCommand {
    pub owner_group_id: GroupId,
    pub name: String,
    pub description: String,
}

/// `None` leaves the field unchanged.
#[derive(Debug, Clone, Default)]
pub struct UpdateProjectMetadataCommand {
    pub name: Option<String>,
    pub description: Option<String>,
}

use domain::ids::GroupId;

#[derive(Debug, Clone)]
pub struct CreateProjectCommand {
    pub owner_group_id: GroupId,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateProjectMetadataCommand {
    pub name: Option<String>,
    pub description: Option<String>,
}

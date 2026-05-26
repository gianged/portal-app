use std::fmt;

use domain::user::SystemRole;

#[derive(Clone)]
pub struct CreateUserCommand {
    pub email: String,
    pub password: String,
    pub full_name: String,
    pub phone: Option<String>,
    pub timezone: String,
    pub system_role: Option<SystemRole>,
}

impl fmt::Debug for CreateUserCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateUserCommand")
            .field("email", &self.email)
            .field("password", &"<redacted>")
            .field("full_name", &self.full_name)
            .field("phone", &self.phone)
            .field("timezone", &self.timezone)
            .field("system_role", &self.system_role)
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdateProfileCommand {
    pub full_name: Option<String>,
    pub phone: Option<String>,
    pub timezone: Option<String>,
    pub avatar_storage_key: Option<String>,
}

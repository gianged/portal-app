use crate::{
    dto::service_account::CreateServiceAccountRequest,
    errors::SharedError,
    validation::{
        Validate,
        common::{self, NAME_MAX, NAME_MIN},
    },
};

impl Validate for CreateServiceAccountRequest {
    fn validate(&self) -> Result<(), SharedError> {
        common::len_range("Service account name", &self.name, NAME_MIN, NAME_MAX)?;
        if self.scopes.is_empty() {
            return Err(SharedError::Validation(
                "At least one scope is required".to_owned(),
            ));
        }
        Ok(())
    }
}

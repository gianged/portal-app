use crate::{
    dto::notification::MarkReadRequest,
    errors::SharedError,
    validation::{
        Validate,
        common::{self, NOTIFICATION_BATCH_MAX},
    },
};

impl Validate for MarkReadRequest {
    fn validate(&self) -> Result<(), SharedError> {
        common::max_items(
            "Notification ids",
            self.notification_ids.len(),
            NOTIFICATION_BATCH_MAX,
        )
    }
}

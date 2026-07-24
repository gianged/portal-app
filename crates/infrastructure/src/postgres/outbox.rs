use sqlx::{Postgres, Transaction};

use domain::{error::RepositoryError, repository::OutboxRecord};

use crate::postgres::mappers;

/// Appends outbox rows inside the caller's entity transaction, so an audited
/// event commits or rolls back with the row it describes.
pub(crate) async fn write(
    tx: &mut Transaction<'_, Postgres>,
    records: &[OutboxRecord],
) -> Result<(), RepositoryError> {
    for r in records {
        sqlx::query!(
            r#"INSERT INTO audit.outbox_events (id, topic, payload) VALUES ($1, $2, $3)"#,
            r.id,
            r.topic,
            r.payload,
        )
        .execute(&mut **tx)
        .await
        .map_err(mappers::map_pg_error)?;
    }
    Ok(())
}

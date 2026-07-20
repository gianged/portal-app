use domain::error::RepositoryError;

use crate::error::{Error, Result};

const MAX_ATTEMPTS: u32 = 3;

/// Runs a load -> guard -> save closure, retrying on [`RepositoryError::Stale`]
/// up to 3 attempts. Each retry reloads inside `op`, so a losing writer fails
/// its guard on the fresh state (a clean conflict) while disjoint field patches
/// reapply and merge silently.
pub async fn retry_stale<T, F, Fut>(mut op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    for _ in 1..MAX_ATTEMPTS {
        match op().await {
            Err(Error::Repository(RepositoryError::Stale)) => {}
            other => return other,
        }
    }
    op().await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn stale<T>() -> Result<T> {
        Err(Error::Repository(RepositoryError::Stale))
    }

    #[tokio::test]
    async fn succeeds_after_stale_retries() {
        let calls = AtomicU32::new(0);
        let result = retry_stale(|| async {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                stale()
            } else {
                Ok(42)
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts() {
        let calls = AtomicU32::new(0);
        let result: Result<()> = retry_stale(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            stale()
        })
        .await;
        assert!(matches!(
            result,
            Err(Error::Repository(RepositoryError::Stale))
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn non_stale_errors_do_not_retry() {
        let calls = AtomicU32::new(0);
        let result: Result<()> = retry_stale(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(Error::Forbidden)
        })
        .await;
        assert!(matches!(result, Err(Error::Forbidden)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

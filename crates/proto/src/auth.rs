//! Static-token auth for the internal gRPC plane. Both binaries share these
//! interceptors so the check and the header format live in one place. mTLS can
//! replace them later without changing call sites.

use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue, errors::InvalidMetadataValue},
    service::Interceptor,
};

const AUTHORIZATION: &str = "authorization";

/// Server interceptor: rejects UNAUTHENTICATED unless the request carries the
/// expected `Bearer` token.
#[derive(Clone)]
pub struct RequireToken {
    expected: String,
}

impl RequireToken {
    #[must_use]
    pub fn new(token: &str) -> Self {
        Self {
            expected: format!("Bearer {token}"),
        }
    }
}

impl Interceptor for RequireToken {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        let presented = request
            .metadata()
            .get(AUTHORIZATION)
            .map(MetadataValue::as_bytes)
            .unwrap_or_default();
        if constant_time_eq(presented, self.expected.as_bytes()) {
            Ok(request)
        } else {
            Err(Status::unauthenticated("invalid internal token"))
        }
    }
}

/// Client interceptor: attaches the shared `Bearer` token to every call.
#[derive(Clone)]
pub struct AttachToken {
    value: MetadataValue<Ascii>,
}

impl AttachToken {
    /// # Errors
    /// Fails if the token contains non-ASCII bytes.
    pub fn new(token: &str) -> Result<Self, InvalidMetadataValue> {
        Ok(Self {
            value: format!("Bearer {token}").parse()?,
        })
    }
}

impl Interceptor for AttachToken {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        request
            .metadata_mut()
            .insert(AUTHORIZATION, self.value.clone());
        Ok(request)
    }
}

/// Compares without short-circuiting on the first mismatched byte. Length still
/// leaks, which is fine for a fixed-size internal token.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_token_accepts_matching_bearer() {
        let mut interceptor = RequireToken::new("secret");
        let mut req = Request::new(());
        req.metadata_mut()
            .insert(AUTHORIZATION, "Bearer secret".parse().unwrap());
        assert!(interceptor.call(req).is_ok());
    }

    #[test]
    fn require_token_rejects_missing_or_wrong() {
        let mut interceptor = RequireToken::new("secret");
        assert!(interceptor.call(Request::new(())).is_err());

        let mut req = Request::new(());
        req.metadata_mut()
            .insert(AUTHORIZATION, "Bearer nope".parse().unwrap());
        assert!(interceptor.call(req).is_err());
    }

    #[test]
    fn attach_token_sets_the_header_require_token_expects() {
        let mut attach = AttachToken::new("secret").unwrap();
        let mut require = RequireToken::new("secret");
        let req = attach.call(Request::new(())).unwrap();
        assert!(require.call(req).is_ok());
    }
}

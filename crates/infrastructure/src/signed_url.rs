//! HMAC-signed, time-limited URLs for the local file store.
//!
//! [`crate::local_storage::LocalStorage::presign_get`] appends `?exp=..&sig=..`
//! to a `/files/{key}` URL; the server's download handler verifies the pair
//! before serving the bytes. The signature binds the storage key to an expiry,
//! so a leaked link stops working after its TTL and cannot be retargeted at a
//! different key. S3-shaped backends would presign with the SDK instead; this is
//! the equivalent for the on-disk store.

use std::fmt::Write as _;
use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use time::OffsetDateTime;

type HmacSha256 = Hmac<Sha256>;

/// Signs and verifies file-download URLs with an embedded expiry. Holds the
/// shared HMAC key (the composition root passes the deployment secret).
pub struct SignedUrl {
    key: Box<[u8]>,
}

impl SignedUrl {
    #[must_use]
    pub fn new(secret: &[u8]) -> Self {
        Self { key: secret.into() }
    }

    /// Returns `(exp, sig)` where `exp` is the Unix-seconds expiry (`now + ttl`,
    /// saturating) and `sig` is the lowercase-hex HMAC-SHA256 of `key|exp`.
    #[must_use]
    pub fn sign(&self, key: &str, ttl: Duration, now: OffsetDateTime) -> (i64, String) {
        let ttl_secs = i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX);
        let exp = now.unix_timestamp().saturating_add(ttl_secs);
        (exp, self.tag(key, exp))
    }

    /// Verifies `sig` against `key|exp` in constant time and confirms the link
    /// has not expired (`exp > now`). Returns `false` on any mismatch, malformed
    /// hex, or expiry.
    #[must_use]
    pub fn verify(&self, key: &str, exp: i64, sig: &str, now: OffsetDateTime) -> bool {
        if exp <= now.unix_timestamp() {
            return false;
        }
        let Some(provided) = decode_hex(sig) else {
            return false;
        };
        let mut mac = self.mac();
        mac.update(message(key, exp).as_bytes());
        mac.verify_slice(&provided).is_ok()
    }

    fn tag(&self, key: &str, exp: i64) -> String {
        let mut mac = self.mac();
        mac.update(message(key, exp).as_bytes());
        encode_hex(&mac.finalize().into_bytes())
    }

    fn mac(&self) -> HmacSha256 {
        // HMAC accepts a key of any length, so this never errors.
        HmacSha256::new_from_slice(&self.key).expect("HMAC accepts any key length")
    }
}

/// Canonical signed message: key and expiry on separate lines so neither field
/// can bleed into the other.
fn message(key: &str, exp: i64) -> String {
    format!("{key}\n{exp}")
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Writing to a String is infallible.
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: Duration = Duration::from_secs(3600);

    fn at(ts: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(ts).expect("valid timestamp")
    }

    #[test]
    fn roundtrip_verifies() {
        let signer = SignedUrl::new(b"secret-key");
        let (exp, sig) = signer.sign("req/1/file.png", TTL, at(1_700_000_000));
        assert!(signer.verify("req/1/file.png", exp, &sig, at(1_700_000_000)));
    }

    #[test]
    fn expired_link_is_rejected() {
        let signer = SignedUrl::new(b"secret-key");
        let (exp, sig) = signer.sign("k", TTL, at(1_700_000_000));
        assert!(!signer.verify("k", exp, &sig, at(exp + 1)));
    }

    #[test]
    fn tampered_key_is_rejected() {
        let signer = SignedUrl::new(b"secret-key");
        let (exp, sig) = signer.sign("k", TTL, at(1_700_000_000));
        assert!(!signer.verify("k2", exp, &sig, at(1_700_000_000)));
    }

    #[test]
    fn tampered_signature_is_rejected() {
        let signer = SignedUrl::new(b"secret-key");
        let (exp, _sig) = signer.sign("k", TTL, at(1_700_000_000));
        assert!(!signer.verify("k", exp, "deadbeef", at(1_700_000_000)));
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let a = SignedUrl::new(b"key-a");
        let b = SignedUrl::new(b"key-b");
        let (exp, sig) = a.sign("k", TTL, at(1_700_000_000));
        assert!(!b.verify("k", exp, &sig, at(1_700_000_000)));
    }

    #[test]
    fn malformed_hex_is_rejected() {
        let signer = SignedUrl::new(b"secret-key");
        let (exp, _) = signer.sign("k", TTL, at(1_700_000_000));
        assert!(!signer.verify("k", exp, "zz", at(1_700_000_000)));
        assert!(!signer.verify("k", exp, "abc", at(1_700_000_000)));
    }
}

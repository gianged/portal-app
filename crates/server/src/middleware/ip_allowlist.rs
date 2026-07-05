//! Network gate: rejects any client whose resolved IP falls outside the configured
//! allowlist with 403, before auth or any handler runs. Toggled by
//! `IP_ALLOWLIST_ENABLED` (default on); the CIDR set comes from `IP_ALLOWLIST` and
//! defaults to loopback + private ranges, so LAN and VPN clients pass unconfigured.
//! When the immediate peer is a trusted reverse proxy (`TRUSTED_PROXIES`), the
//! client is the rightmost non-trusted `X-Forwarded-For` hop; from any other peer
//! the header is ignored, since it is trivially spoofable.
//! Fails closed: no peer address, or a trusted peer with an unusable header, is
//! rejected — an unverifiable network cannot be trusted.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use application::Error;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use ipnet::IpNet;

use crate::{app::AppState, error::AppError};

/// The trusted-proxy-resolved client IP, inserted into request extensions by
/// [`enforce`] so downstream middleware (the login rate limiter) judges the
/// same address the gate did.
#[derive(Clone, Copy)]
pub struct ClientIp(pub IpAddr);

/// Allowed source networks + trusted proxies + enable flag. Held in `AppState`,
/// populated from `Config`. `Arc<[IpNet]>` keeps `AppState` cheap to clone.
#[derive(Clone)]
pub struct IpAllowlist {
    pub enabled: bool,
    pub nets: Arc<[IpNet]>,
    pub trusted_proxies: Arc<[IpNet]>,
}

impl IpAllowlist {
    fn allows(&self, ip: IpAddr) -> bool {
        self.nets.iter().any(|n| n.contains(&ip))
    }

    fn is_trusted_proxy(&self, ip: IpAddr) -> bool {
        self.trusted_proxies.iter().any(|n| n.contains(&ip))
    }

    /// The IP the gate judges: the peer itself, or, when the peer is a trusted
    /// proxy, the rightmost `X-Forwarded-For` hop that is not itself a trusted
    /// proxy. `None` when a trusted peer carries a missing, malformed, or
    /// all-trusted header (fail closed).
    fn client_ip(&self, peer: IpAddr, headers: &HeaderMap) -> Option<IpAddr> {
        if !self.is_trusted_proxy(peer) {
            return Some(peer);
        }
        let forwarded = headers.get("x-forwarded-for")?.to_str().ok()?;
        // Right-to-left: proxies append, so trusted hops are skipped until the
        // first address a trusted proxy actually saw. A malformed hop poisons
        // the chain rather than being skipped.
        for hop in forwarded.rsplit(',') {
            let ip: IpAddr = hop.trim().parse().ok()?;
            if !self.is_trusted_proxy(ip) {
                return Some(ip);
            }
        }
        None
    }
}

/// Global middleware: passes the request through when the gate is disabled or the
/// resolved client IP is allowlisted, otherwise returns 403. Fails closed when no
/// peer address is attached or a trusted proxy forwards an unusable chain.
pub async fn enforce(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if !state.ip_allowlist.enabled {
        return Ok(next.run(req).await);
    }

    let Some(peer) = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
    else {
        tracing::warn!("ip allowlist: no peer address, rejecting (fail closed)");
        return Err(Error::Forbidden.into());
    };

    match state.ip_allowlist.client_ip(peer, req.headers()) {
        Some(ip) if state.ip_allowlist.allows(ip) => {
            req.extensions_mut().insert(ClientIp(ip));
            Ok(next.run(req).await)
        }
        Some(ip) => {
            tracing::warn!(peer = %peer, client_ip = %ip, "ip allowlist: rejected out-of-network client");
            Err(Error::Forbidden.into())
        }
        None => {
            tracing::warn!(peer = %peer, "ip allowlist: unusable X-Forwarded-For from trusted proxy, rejecting (fail closed)");
            Err(Error::Forbidden.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn allowlist(nets: &[&str]) -> IpAllowlist {
        with_proxies(nets, &[])
    }

    fn with_proxies(nets: &[&str], proxies: &[&str]) -> IpAllowlist {
        IpAllowlist {
            enabled: true,
            nets: nets.iter().map(|s| s.parse().unwrap()).collect(),
            trusted_proxies: proxies.iter().map(|s| s.parse().unwrap()).collect(),
        }
    }

    fn xff(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        headers
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn allows_in_range_v4() {
        assert!(allowlist(&["10.0.0.0/8"]).allows(ip("10.1.2.3")));
    }

    #[test]
    fn rejects_out_of_range_v4() {
        assert!(!allowlist(&["10.0.0.0/8"]).allows(ip("192.168.1.1")));
    }

    #[test]
    fn allows_in_range_v6() {
        assert!(allowlist(&["fc00::/7"]).allows(ip("fd12::1")));
    }

    #[test]
    fn untrusted_peer_ignores_spoofed_header() {
        let gate = with_proxies(&["10.0.0.0/8"], &["172.16.0.1/32"]);
        let client = gate.client_ip(ip("203.0.113.9"), &xff("10.1.2.3"));
        assert_eq!(client, Some(ip("203.0.113.9")));
    }

    #[test]
    fn trusted_proxy_resolves_forwarded_client() {
        let gate = with_proxies(&["10.0.0.0/8"], &["172.16.0.1/32"]);
        let client = gate.client_ip(ip("172.16.0.1"), &xff("10.1.2.3"));
        assert_eq!(client, Some(ip("10.1.2.3")));
    }

    #[test]
    fn trusted_proxy_chain_skips_trusted_hops() {
        let gate = with_proxies(&["10.0.0.0/8"], &["172.16.0.0/24"]);
        let client = gate.client_ip(ip("172.16.0.1"), &xff("10.1.2.3, 172.16.0.2"));
        assert_eq!(client, Some(ip("10.1.2.3")));
    }

    #[test]
    fn trusted_proxy_without_header_fails_closed() {
        let gate = with_proxies(&["10.0.0.0/8"], &["172.16.0.1/32"]);
        assert_eq!(gate.client_ip(ip("172.16.0.1"), &HeaderMap::new()), None);
    }

    #[test]
    fn trusted_proxy_with_garbage_header_fails_closed() {
        let gate = with_proxies(&["10.0.0.0/8"], &["172.16.0.1/32"]);
        assert_eq!(gate.client_ip(ip("172.16.0.1"), &xff("not-an-ip")), None);
        assert_eq!(
            gate.client_ip(ip("172.16.0.1"), &xff("10.1.2.3, garbage")),
            None
        );
    }

    #[test]
    fn all_trusted_hops_fail_closed() {
        let gate = with_proxies(&["10.0.0.0/8"], &["172.16.0.0/24"]);
        assert_eq!(
            gate.client_ip(ip("172.16.0.1"), &xff("172.16.0.3, 172.16.0.2")),
            None
        );
    }
}

//! Network gate: rejects any client whose peer IP falls outside the configured
//! allowlist with 403, before auth or any handler runs. Toggled by
//! `IP_ALLOWLIST_ENABLED` (default on); the CIDR set comes from `IP_ALLOWLIST` and
//! defaults to loopback + private ranges, so LAN and VPN clients pass unconfigured.
//! Fails closed: with no peer address the client is rejected, since an unverifiable
//! network cannot be trusted.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use application::Error;
use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};
use ipnet::IpNet;

use crate::{app::AppState, error::AppError};

/// Allowed source networks + enable flag. Held in `AppState`, populated from
/// `Config`. `Arc<[IpNet]>` keeps `AppState` cheap to clone.
#[derive(Clone)]
pub struct IpAllowlist {
    pub enabled: bool,
    pub nets: Arc<[IpNet]>,
}

impl IpAllowlist {
    fn allows(&self, ip: IpAddr) -> bool {
        self.nets.iter().any(|n| n.contains(&ip))
    }
}

/// Global middleware: passes the request through when the gate is disabled or the
/// peer IP is allowlisted, otherwise returns 403. Fails closed when no peer address
/// is attached.
pub async fn enforce(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if !state.ip_allowlist.enabled {
        return Ok(next.run(req).await);
    }

    // TODO: behind a reverse proxy, take the client from X-Forwarded-For only when the
    // immediate peer is a trusted proxy. Trusting the header unconditionally is spoofable.
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());

    match peer {
        Some(ip) if state.ip_allowlist.allows(ip) => Ok(next.run(req).await),
        Some(ip) => {
            tracing::warn!(client_ip = %ip, "ip allowlist: rejected out-of-network client");
            Err(Error::Forbidden.into())
        }
        None => {
            tracing::warn!("ip allowlist: no peer address, rejecting (fail closed)");
            Err(Error::Forbidden.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowlist(nets: &[&str]) -> IpAllowlist {
        IpAllowlist {
            enabled: true,
            nets: nets.iter().map(|s| s.parse().unwrap()).collect(),
        }
    }

    #[test]
    fn allows_in_range_v4() {
        assert!(allowlist(&["10.0.0.0/8"]).allows("10.1.2.3".parse().unwrap()));
    }

    #[test]
    fn rejects_out_of_range_v4() {
        assert!(!allowlist(&["10.0.0.0/8"]).allows("192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn allows_in_range_v6() {
        assert!(allowlist(&["fc00::/7"]).allows("fd12::1".parse().unwrap()));
    }
}

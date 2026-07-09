//! SSRF guard: reject URLs that resolve to non-public IP ranges.
//!
//! The clip endpoint fetches arbitrary URLs server-side. To bound the exposure
//! (per [ADR 0020](../../docs/adr/0020-web-clipper.md) §6), every host is
//! resolved and every candidate address is checked. Any address in a
//! loopback / private / link-local / otherwise non-public range causes the
//! fetch to be refused.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use crate::error::ClipError;

/// Returns true if the address must never be fetched from the daemon.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        // 100.64.0.0/10 CGNAT / shared address space.
        || (o[0] == 100 && (o[1] & 0xc0) == 64)
        // 192.0.0.0/24 IETF protocol assignments.
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        // 198.18.0.0/15 benchmarking.
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
        // 240.0.0.0/4 reserved (excluding broadcast handled above).
        || o[0] >= 240
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    let seg = ip.segments();
    // Unique local addresses fc00::/7.
    if (seg[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // Link-local unicast fe80::/10.
    if (seg[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // IPv4-mapped ::ffff:0:0/96 — validate the embedded v4 address.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_v4(v4);
    }
    false
}

/// Resolve `host:port` and return only public socket addresses.
///
/// Errors if the host cannot be resolved, or if every resolved address is in a
/// blocked range (which is treated as an SSRF attempt).
pub fn resolve_public_addrs(host: &str, port: u16) -> Result<Vec<SocketAddr>, ClipError> {
    // A bare IP literal host is validated directly.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(ClipError::Blocked(format!("ip {ip} is not public")));
        }
        return Ok(vec![SocketAddr::new(ip, port)]);
    }

    let resolved: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| ClipError::Fetch(format!("dns resolution failed for {host}: {e}")))?
        .collect();

    if resolved.is_empty() {
        return Err(ClipError::Fetch(format!("no addresses for {host}")));
    }

    let public: Vec<SocketAddr> = resolved
        .into_iter()
        .filter(|addr| !is_blocked_ip(addr.ip()))
        .collect();

    if public.is_empty() {
        return Err(ClipError::Blocked(format!(
            "host {host} resolves only to non-public addresses"
        )));
    }

    Ok(public)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_loopback_and_private_v4() {
        assert!(is_blocked_ip("127.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("10.0.0.5".parse().unwrap()));
        assert!(is_blocked_ip("192.168.1.1".parse().unwrap()));
        assert!(is_blocked_ip("172.16.0.1".parse().unwrap()));
        assert!(is_blocked_ip("169.254.10.1".parse().unwrap())); // link-local
        assert!(is_blocked_ip("100.64.0.1".parse().unwrap())); // CGNAT
        assert!(is_blocked_ip("0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn allows_public_v4() {
        assert!(!is_blocked_ip("1.1.1.1".parse().unwrap()));
        assert!(!is_blocked_ip("93.184.216.34".parse().unwrap())); // example.com
        assert!(!is_blocked_ip("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn blocks_ipv6_loopback_ula_linklocal() {
        assert!(is_blocked_ip("::1".parse().unwrap()));
        assert!(is_blocked_ip("fc00::1".parse().unwrap()));
        assert!(is_blocked_ip("fe80::1".parse().unwrap()));
        assert!(is_blocked_ip("::".parse().unwrap()));
    }

    #[test]
    fn blocks_ipv4_mapped_v6_pointing_at_private() {
        assert!(is_blocked_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("::ffff:10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn allows_public_ipv6() {
        assert!(!is_blocked_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn resolve_rejects_ip_literal_loopback() {
        let err = resolve_public_addrs("127.0.0.1", 80).unwrap_err();
        assert!(matches!(err, ClipError::Blocked(_)));
    }

    #[test]
    fn resolve_accepts_public_ip_literal() {
        let addrs = resolve_public_addrs("1.1.1.1", 443).unwrap();
        assert_eq!(
            addrs,
            vec![SocketAddr::new("1.1.1.1".parse().unwrap(), 443)]
        );
    }
}

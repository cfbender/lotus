//! Where a ManaVault share link may be fetched from.
//!
//! A pasted link names an arbitrary origin, so before any request the host
//! is resolved and every answer must be a public address, or the host or
//! address must be on an operator allowlist. The policy here is ManaVault's
//! (`Trade.ListSource.ManaVaultRemote`), which is a superset of
//! the-gathering's `Decklists.Destination` checks.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use url::Url;

/// `http` or `https`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    /// Plain HTTP.
    Http,
    /// HTTPS.
    Https,
}

impl Scheme {
    /// The scheme's default port.
    #[must_use]
    pub fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }

    /// The scheme as text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// The scheme, host, and port of a URL, with the host lowercased. The
/// path, query, userinfo, and fragment of the original link are discarded,
/// so an origin can only ever be combined with a path this crate chooses.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Origin {
    /// `http` or `https`.
    pub scheme: Scheme,
    /// The lowercased host. IPv6 literals are stored without brackets.
    pub host: String,
    /// The explicit port, when the URL names one that differs from the
    /// scheme's default.
    pub port: Option<u16>,
}

impl Origin {
    /// The origin of an absolute `http(s)` URL with a host.
    #[must_use]
    pub fn of(url: &Url) -> Option<Self> {
        let scheme = match url.scheme() {
            "http" => Scheme::Http,
            "https" => Scheme::Https,
            _ => return None,
        };
        let host = match url.host()? {
            url::Host::Domain(domain) => domain.to_ascii_lowercase(),
            url::Host::Ipv4(address) => address.to_string(),
            url::Host::Ipv6(address) => address.to_string(),
        };
        if host.is_empty() {
            return None;
        }
        Some(Self {
            scheme,
            host,
            port: url.port(),
        })
    }

    /// Parses an absolute `http(s)` URL and keeps its origin.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Url::parse(value.trim()).ok().and_then(|url| Self::of(&url))
    }

    /// The host as an IP address, when it is a literal.
    #[must_use]
    pub fn ip_literal(&self) -> Option<IpAddr> {
        self.host.parse().ok()
    }

    /// The port to connect to.
    #[must_use]
    pub fn port_or_default(&self) -> u16 {
        self.port.unwrap_or_else(|| self.scheme.default_port())
    }

    /// The host formatted for a URL or `Host` header: bracketed when IPv6.
    #[must_use]
    pub fn host_for_url(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        }
    }

    /// The HTTP `Host` header value: the host plus the port when it is not
    /// the scheme's default.
    #[must_use]
    pub fn authority(&self) -> String {
        match self.port {
            Some(port) if port != self.scheme.default_port() => {
                format!("{}:{port}", self.host_for_url())
            }
            _ => self.host_for_url(),
        }
    }

    /// `<origin><path>` for a path starting with `/`.
    #[must_use]
    pub fn join(&self, path: &str) -> String {
        format!("{self}{path}")
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme.as_str(), self.authority())
    }
}

/// IPv4 ranges that are never public: unspecified, private, CGNAT,
/// loopback, link-local, protocol assignments, documentation, 6to4 relay,
/// benchmarking, multicast, and reserved.
const IPV4_NON_PUBLIC: [(Ipv4Addr, u8); 15] = [
    (Ipv4Addr::UNSPECIFIED, 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 88, 99, 0), 24),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

/// Global-unicast IPv6 ranges that are still not public: IETF protocol
/// assignments (which include ORCHID and benchmarking), documentation, 6to4,
/// and the newer documentation block.
const IPV6_NON_PUBLIC: [(Ipv6Addr, u8); 4] = [
    (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0xDB8, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0), 16),
    (Ipv6Addr::new(0x3FFF, 0, 0, 0, 0, 0, 0, 0), 20),
];

const IPV6_GLOBAL_UNICAST: (Ipv6Addr, u8) = (Ipv6Addr::new(0x2000, 0, 0, 0, 0, 0, 0, 0), 3);

fn in_v4_cidr(address: Ipv4Addr, network: Ipv4Addr, prefix: u8) -> bool {
    let shift = 32_u32.saturating_sub(u32::from(prefix));
    let mask = if shift >= 32 { 0 } else { u32::MAX << shift };
    (u32::from(address) & mask) == (u32::from(network) & mask)
}

fn in_v6_cidr(address: Ipv6Addr, network: Ipv6Addr, prefix: u8) -> bool {
    let shift = 128_u32.saturating_sub(u32::from(prefix));
    let mask = if shift >= 128 { 0 } else { u128::MAX << shift };
    (u128::from(address) & mask) == (u128::from(network) & mask)
}

/// Whether `address` is inside `network/prefix`. Mixed address families
/// never match.
#[must_use]
pub fn in_cidr(address: IpAddr, network: IpAddr, prefix: u8) -> bool {
    match (address, network) {
        (IpAddr::V4(address), IpAddr::V4(network)) => in_v4_cidr(address, network, prefix),
        (IpAddr::V6(address), IpAddr::V6(network)) => in_v6_cidr(address, network, prefix),
        _ => false,
    }
}

/// Whether an address is publicly routable. IPv4-mapped IPv6 addresses are
/// judged as their IPv4 address; other IPv6 addresses must be global
/// unicast (`2000::/3`) outside the reserved blocks.
#[must_use]
pub fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => IPV4_NON_PUBLIC
            .iter()
            .all(|(network, prefix)| !in_v4_cidr(address, *network, *prefix)),
        IpAddr::V6(address) => match address.to_ipv4_mapped() {
            Some(mapped) => is_public_address(IpAddr::V4(mapped)),
            None => {
                in_v6_cidr(address, IPV6_GLOBAL_UNICAST.0, IPV6_GLOBAL_UNICAST.1)
                    && IPV6_NON_PUBLIC
                        .iter()
                        .all(|(network, prefix)| !in_v6_cidr(address, *network, *prefix))
            }
        },
    }
}

/// Operator-configured destinations that may be fetched even when they are
/// not public: hostnames (compared case-insensitively, ignoring a trailing
/// dot) and CIDR blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowlist {
    hosts: Vec<String>,
    cidrs: Vec<(IpAddr, u8)>,
}

impl Allowlist {
    /// An empty allowlist: only public addresses pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses entries such as `friend.home`, `10.0.0.0/8`, or
    /// `fd12:3456::/64`. Malformed CIDR entries are ignored; entries without
    /// a slash are hostnames.
    #[must_use]
    pub fn parse<'a>(entries: impl IntoIterator<Item = &'a str>) -> Self {
        let mut allowlist = Self::new();
        for entry in entries {
            match entry.split_once('/') {
                Some((address, prefix)) => {
                    if let Some(cidr) = parse_cidr(address, prefix) {
                        allowlist.cidrs.push(cidr);
                    }
                }
                None => allowlist.hosts.push(normalize_host(entry)),
            }
        }
        allowlist
    }

    /// Whether `host` is listed by name.
    #[must_use]
    pub fn allows_host(&self, host: &str) -> bool {
        let host = normalize_host(host);
        self.hosts.contains(&host)
    }

    /// Whether `address` is public or inside a listed CIDR.
    #[must_use]
    pub fn allows_address(&self, address: IpAddr) -> bool {
        is_public_address(address)
            || self
                .cidrs
                .iter()
                .any(|(network, prefix)| in_cidr(address, *network, *prefix))
    }

    /// Whether every resolved address of `host` may be contacted: the host
    /// is listed, or each address is public or inside a listed CIDR. An
    /// empty answer set is never allowed.
    #[must_use]
    pub fn allows(&self, host: &str, addresses: &[IpAddr]) -> bool {
        !addresses.is_empty()
            && (self.allows_host(host)
                || addresses
                    .iter()
                    .all(|address| self.allows_address(*address)))
    }
}

fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn parse_cidr(address: &str, prefix: &str) -> Option<(IpAddr, u8)> {
    let address: IpAddr = address.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    let bits = match address {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    };
    (prefix <= bits).then_some((address, prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    #[test]
    fn origin_keeps_only_scheme_host_port() {
        let origin = Origin::parse("http://Friend.Home:4000/private/path?ignored=yes#f").unwrap();
        assert_eq!(origin.host, "friend.home");
        assert_eq!(origin.port, Some(4000));
        assert_eq!(origin.authority(), "friend.home:4000");
        assert_eq!(origin.to_string(), "http://friend.home:4000");
        assert_eq!(
            origin.join("/share/graphql"),
            "http://friend.home:4000/share/graphql"
        );

        let default_port = Origin::parse("https://other-vault.example:443/x").unwrap();
        assert_eq!(default_port.authority(), "other-vault.example");
        assert_eq!(default_port.port_or_default(), 443);

        let v6 = Origin::parse("http://[fd12:3456::20]/share/decks/t").unwrap();
        assert_eq!(v6.host, "fd12:3456::20");
        assert_eq!(v6.authority(), "[fd12:3456::20]");
        assert_eq!(v6.ip_literal(), Some(ip("fd12:3456::20")));

        assert_eq!(Origin::parse("ftp://example.test/x"), None);
        assert_eq!(Origin::parse("mailto:someone@example.test"), None);
    }

    #[test]
    fn non_public_addresses_are_rejected() {
        for address in [
            "127.0.0.1",
            "10.20.30.40",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.1.1",
            "172.16.0.1",
            "192.168.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "fd12:3456::20",
            "fe80::1",
            "::",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "2001:2::1",
            "2001:20::1",
            "2001:db8::1",
            "2002:7f00:1::1",
            "3fff::1",
            "ff02::1",
        ] {
            assert!(!is_public_address(ip(address)), "{address}");
        }
    }

    #[test]
    fn public_addresses_are_accepted() {
        for address in [
            "93.184.216.34",
            "8.8.8.8",
            "2606:4700:4700::1111",
            "::ffff:93.184.216.34",
            "2001:4860::1",
        ] {
            assert!(is_public_address(ip(address)), "{address}");
        }
    }

    #[test]
    fn allowlist_hosts_and_cidrs() {
        let allowlist =
            Allowlist::parse(["Friend.Home.", "fd12:3456::/64", "bad/entry", "10.0.0.0/33"]);
        assert!(allowlist.allows_host("friend.home"));
        assert!(!allowlist.allows_host("other.home"));
        assert!(allowlist.allows("friend.home", &[ip("192.168.50.24")]));
        assert!(allowlist.allows("x.example", &[ip("fd12:3456::20")]));
        assert!(!allowlist.allows("x.example", &[ip("fd13:3456::20")]));
        assert!(!allowlist.allows("x.example", &[ip("10.1.1.1")]));
        assert!(allowlist.allows("x.example", &[ip("93.184.216.34")]));
        assert!(
            !allowlist.allows(
                "rebinding.example",
                &[ip("93.184.216.34"), ip("169.254.169.254")]
            ),
            "one private answer blocks the whole set"
        );
        assert!(!allowlist.allows("x.example", &[]));
        assert_eq!(Allowlist::new(), Allowlist::default());
    }
}

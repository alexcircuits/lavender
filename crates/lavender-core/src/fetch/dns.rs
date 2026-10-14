//! Resolve once, validate every address, then give that exact set to the connector.
//! No second unguarded lookup or ambient proxy may bypass the public egress policy.

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::error::Error;
use std::fmt;
use std::net::{IpAddr, SocketAddr};

#[derive(Debug)]
pub(super) struct BlockedAddress;

impl fmt::Display for BlockedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("upstream address refused by network policy")
    }
}

impl Error for BlockedAddress {}

/// Conservative public unicast policy, excluding special-purpose and tunnelling ranges.
/// Registry references and explicit development exceptions are in security.md.
pub(super) fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            let reserved = matches!(a, 0 | 10 | 127 | 224..=255)
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192
                    && (b == 168 || (b == 0 && matches!(c, 0 | 2)) || (b == 88 && c == 99)))
                || (a == 198 && (matches!(b, 18 | 19) || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113);
            !reserved
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_public(IpAddr::V4(mapped));
            }
            let [a, b, ..] = ip.segments();
            // Current native global unicast only. Reject translated/tunnel addresses,
            // local/special-purpose assignments, and both documentation prefixes.
            (0x2000..=0x3fff).contains(&a)
                && !(a == 0x2001 && (b <= 0x01ff || b == 0x0db8))
                && a != 0x2002
                && !(a == 0x3fff && b <= 0x0fff)
        }
    }
}

#[derive(Debug)]
pub(super) struct PublicResolver {
    development_hosts: Vec<String>,
}

impl PublicResolver {
    pub(super) fn new(development_hosts: Vec<String>) -> Self {
        Self { development_hosts }
    }

    fn validate(
        &self,
        host: &str,
        addresses: Vec<SocketAddr>,
    ) -> Result<Addrs, Box<dyn Error + Send + Sync>> {
        let development = self.development_hosts.iter().any(|h| h.eq_ignore_ascii_case(host));
        if addresses.is_empty()
            || addresses.len() > 64
            || (!development && addresses.iter().any(|addr| !is_public(addr.ip())))
        {
            return Err(Box::new(BlockedAddress));
        }
        Ok(Box::new(addresses.into_iter()))
    }
}

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let resolver = Self::new(self.development_hosts.clone());
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0)).await?.take(65).collect();
            resolver.validate(&host, addresses)
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn refuses_private_reserved_and_embedded_private_addresses() {
        for value in [
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.2",
            "169.254.169.254",
            "172.31.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "192.168.0.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "64:ff9b::a00:1",
            "2001:db8::1",
            "2001::1",
            "2002:a00:1::1",
            "3fff::1",
        ] {
            assert!(!is_public(value.parse().unwrap()), "{value}");
        }
        for value in [
            "8.8.8.8",
            "1.1.1.1",
            "100.128.0.1",
            "172.32.0.1",
            "2001:4860:4860::8888",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public(value.parse().unwrap()), "{value}");
        }
    }

    #[test]
    fn rejects_mixed_answers_and_rebinding_then_passes_exact_validated_set() {
        let resolver = PublicResolver::new(vec![]);
        let public: SocketAddr = "8.8.8.8:0".parse().unwrap();
        let private: SocketAddr = "127.0.0.1:0".parse().unwrap();
        assert_eq!(
            resolver.validate("source.example", vec![public]).unwrap().collect::<Vec<_>>(),
            vec![public]
        );
        assert!(resolver.validate("source.example", vec![public, private]).is_err());
        assert!(resolver.validate("source.example", vec![private]).is_err());
        assert!(resolver.validate("source.example", vec![]).is_err());
        assert!(resolver.validate("source.example", vec![public; 65]).is_err());
        let development = PublicResolver::new(vec!["localhost".into()]);
        assert!(development.validate("LOCALHOST", vec![private]).is_ok());
        assert!(development.validate("localhost.other", vec![private]).is_err());
    }
}

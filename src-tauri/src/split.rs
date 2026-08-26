use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use crate::parse::ProxyLine;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Subnet(Ipv4Addr);

impl Subnet {
    pub fn from_host(host: Ipv4Addr) -> Self {
        let octets = host.octets();
        Self(Ipv4Addr::new(octets[0], octets[1], octets[2], 0))
    }

    pub fn cidr(self) -> String {
        format!("{}/24", self.0)
    }

    pub fn network(self) -> Ipv4Addr {
        self.0
    }

    pub fn parse_cidr(cidr: &str) -> Option<Self> {
        let (ip, prefix) = cidr.split_once('/')?;
        if prefix != "24" {
            return None;
        }
        let host: Ipv4Addr = ip.parse().ok()?;
        let subnet = Self::from_host(host);
        if subnet.cidr() != cidr {
            return None;
        }
        Some(subnet)
    }
}

pub struct Bucket {
    pub subnet: Subnet,
    pub proxies: Vec<ProxyLine>,
}

pub fn by_slash24(proxies: Vec<ProxyLine>) -> Vec<Bucket> {
    let mut groups: BTreeMap<Subnet, Vec<ProxyLine>> = BTreeMap::new();
    for proxy in proxies {
        groups
            .entry(Subnet::from_host(proxy.host))
            .or_default()
            .push(proxy);
    }
    groups
        .into_iter()
        .map(|(subnet, proxies)| Bucket { subnet, proxies })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy(host: &str) -> ProxyLine {
        ProxyLine {
            host: host.parse().unwrap(),
            port: 8080,
            username: "user".into(),
            password: "pass".into(),
            source: format!("{host}:8080:user:pass"),
        }
    }

    #[test]
    fn from_host_masks_to_slash24() {
        let subnet = Subnet::from_host("192.0.2.17".parse().unwrap());
        assert_eq!(subnet.cidr(), "192.0.2.0/24");
        assert_eq!(subnet.network(), "192.0.2.0".parse::<Ipv4Addr>().unwrap());
        assert_eq!(
            Subnet::parse_cidr("192.0.2.0/24").unwrap().cidr(),
            "192.0.2.0/24"
        );
        assert!(Subnet::parse_cidr("192.0.2.17/24").is_none());
        assert!(Subnet::parse_cidr("192.0.2.0/16").is_none());
    }

    #[test]
    fn by_slash24_groups_and_sorts_by_network() {
        let buckets = by_slash24(vec![
            proxy("198.51.100.2"),
            proxy("192.0.2.10"),
            proxy("192.0.2.11"),
        ]);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].subnet.cidr(), "192.0.2.0/24");
        assert_eq!(buckets[0].proxies.len(), 2);
        assert_eq!(buckets[1].subnet.cidr(), "198.51.100.0/24");
        assert_eq!(buckets[1].proxies.len(), 1);
    }
}

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::health::Health;
use crate::measurement::Probe;
use crate::proxy::ProxyLine;
use crate::subnet::{Group, Subnet};

#[derive(Clone)]
pub struct Bucket {
    pub subnet: Subnet,
    pub proxies: Vec<ProxyLine>,
    pub country: Option<String>,
    pub health: Option<Health>,
    pub probe: Option<Probe>,
}

impl Bucket {
    pub fn new(subnet: Subnet, proxies: Vec<ProxyLine>, country: Option<String>) -> Self {
        let mut bucket = Self {
            subnet,
            proxies: Vec::new(),
            country,
            health: None,
            probe: None,
        };
        bucket.absorb(proxies);
        bucket
    }

    pub fn absorb(&mut self, incoming: Vec<ProxyLine>) -> usize {
        let before = self.proxies.len();
        for proxy in incoming {
            if !self
                .proxies
                .iter()
                .any(|known| known.source == proxy.source)
            {
                self.proxies.push(proxy);
            }
        }
        self.proxies.len() - before
    }

    pub fn clear_results(&mut self) {
        self.health = None;
        self.probe = None;
    }
}

pub struct Merge {
    pub added: Vec<String>,
    pub grown: Vec<String>,
}

#[derive(Clone, Default)]
pub struct Inventory {
    buckets: BTreeMap<Subnet, Bucket>,
}

impl Inventory {
    pub fn restore(buckets: Vec<Bucket>) -> Self {
        Self {
            buckets: buckets
                .into_iter()
                .map(|bucket| (bucket.subnet, bucket))
                .collect(),
        }
    }

    pub fn merge(&mut self, groups: Vec<Group>, countries: &HashMap<String, String>) -> Merge {
        let mut merge = Merge {
            added: Vec::new(),
            grown: Vec::new(),
        };
        for group in groups {
            let cidr = group.subnet.cidr();
            match self.buckets.get_mut(&group.subnet) {
                Some(bucket) => {
                    if bucket.country.is_none() {
                        bucket.country = countries.get(&cidr).cloned();
                    }
                    if bucket.absorb(group.proxies) > 0 {
                        bucket.clear_results();
                        merge.grown.push(cidr);
                    }
                }
                None => {
                    let country = countries.get(&cidr).cloned();
                    self.buckets.insert(
                        group.subnet,
                        Bucket::new(group.subnet, group.proxies, country),
                    );
                    merge.added.push(cidr);
                }
            }
        }
        merge
    }

    pub fn remove(&mut self, subnets: &[Subnet]) {
        for subnet in subnets {
            self.buckets.remove(subnet);
        }
    }

    pub fn record_check(&mut self, subnet: Subnet, health: Health) {
        if let Some(bucket) = self.buckets.get_mut(&subnet) {
            bucket.health = Some(health);
        }
    }

    pub fn record_probe(&mut self, subnet: Subnet, probe: Probe) {
        if let Some(bucket) = self.buckets.get_mut(&subnet) {
            bucket.health = Some(Health::new(probe.ok, probe.tested, probe.at));
            bucket.probe = Some(probe);
        }
    }

    pub fn record_countries(&mut self, countries: &HashMap<String, String>) {
        for (cidr, country) in countries {
            let Some(subnet) = Subnet::parse_cidr(cidr) else {
                continue;
            };
            if let Some(bucket) = self.buckets.get_mut(&subnet) {
                bucket.country = Some(country.clone());
            }
        }
    }

    pub fn get(&self, subnet: Subnet) -> Option<&Bucket> {
        self.buckets.get(&subnet)
    }

    pub fn buckets(&self) -> Vec<Bucket> {
        self.buckets.values().cloned().collect()
    }

    pub fn scope(&self, cidrs: Option<Vec<String>>) -> Result<Vec<Bucket>, String> {
        let Some(cidrs) = cidrs else {
            return Ok(self.buckets());
        };
        let subnets = parse_subnets(&cidrs)?;
        subnets
            .into_iter()
            .map(|subnet| {
                self.buckets
                    .get(&subnet)
                    .cloned()
                    .ok_or_else(|| "Unknown subnet.".to_string())
            })
            .collect()
    }
}

pub fn parse_subnets(cidrs: &[String]) -> Result<BTreeSet<Subnet>, String> {
    if cidrs.is_empty() {
        return Err("Select at least one subnet.".into());
    }
    cidrs
        .iter()
        .map(|cidr| Subnet::parse_cidr(cidr).ok_or_else(|| "Unknown subnet.".to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subnet::by_slash24;

    fn proxy(host: &str) -> ProxyLine {
        ProxyLine {
            host: host.parse().unwrap(),
            port: 8080,
            username: "user".into(),
            password: "pass".into(),
            source: format!("{host}:8080:user:pass"),
        }
    }

    fn spaced(host: &str) -> ProxyLine {
        let mut padded = proxy(host);
        padded.source = format!("  {host}:8080:user:pass  ");
        padded
    }

    fn probe(at: u64, ok: usize, tested: usize) -> Probe {
        Probe {
            at,
            ok,
            tested,
            connect_p50: Some(10.0),
            connect_p95: Some(20.0),
            ttfb_p50: Some(30.0),
            ttfb_p95: Some(40.0),
        }
    }

    fn subnet(host: &str) -> Subnet {
        Subnet::from_host(host.parse().unwrap())
    }

    fn only_bucket(inventory: &Inventory) -> Bucket {
        inventory.buckets().into_iter().next().unwrap()
    }

    #[test]
    fn merge_reports_added_and_grown_subnets() {
        let mut inventory = Inventory::default();
        let first = inventory.merge(
            by_slash24(vec![proxy("192.0.2.10"), proxy("198.51.100.2")]),
            &HashMap::new(),
        );
        assert_eq!(first.added, vec!["192.0.2.0/24", "198.51.100.0/24"]);
        assert!(first.grown.is_empty());
        let second = inventory.merge(
            by_slash24(vec![
                proxy("192.0.2.10"),
                proxy("192.0.2.11"),
                spaced("192.0.2.10"),
                proxy("203.0.113.5"),
            ]),
            &HashMap::new(),
        );
        assert_eq!(second.added, vec!["203.0.113.0/24"]);
        assert_eq!(second.grown, vec!["192.0.2.0/24"]);
        let grown = inventory.get(subnet("192.0.2.10")).unwrap();
        assert_eq!(grown.proxies.len(), 3);
    }

    #[test]
    fn merge_keeps_known_country_and_fills_missing_one() {
        let mut inventory = Inventory::default();
        let mut countries = HashMap::new();
        countries.insert("192.0.2.0/24".to_string(), "FR".to_string());
        inventory.merge(by_slash24(vec![proxy("192.0.2.10")]), &countries);
        inventory.merge(by_slash24(vec![proxy("198.51.100.2")]), &HashMap::new());
        let mut later = HashMap::new();
        later.insert("192.0.2.0/24".to_string(), "US".to_string());
        later.insert("198.51.100.0/24".to_string(), "DE".to_string());
        inventory.merge(
            by_slash24(vec![proxy("192.0.2.10"), proxy("198.51.100.2")]),
            &later,
        );
        let buckets = inventory.buckets();
        assert_eq!(buckets[0].country.as_deref(), Some("FR"));
        assert_eq!(buckets[1].country.as_deref(), Some("DE"));
    }

    #[test]
    fn merge_clears_health_and_probe_when_a_subnet_grows() {
        let mut inventory = Inventory::default();
        inventory.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        inventory.record_probe(subnet("192.0.2.10"), probe(42, 1, 1));
        assert!(only_bucket(&inventory).health.is_some());
        assert!(only_bucket(&inventory).probe.is_some());
        inventory.merge(by_slash24(vec![proxy("192.0.2.11")]), &HashMap::new());
        let bucket = only_bucket(&inventory);
        assert_eq!(bucket.health, None);
        assert_eq!(bucket.probe, None);
    }

    #[test]
    fn merge_keeps_results_when_nothing_new_arrives() {
        let mut inventory = Inventory::default();
        inventory.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        inventory.record_probe(subnet("192.0.2.10"), probe(42, 1, 1));
        let merge = inventory.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        assert!(merge.grown.is_empty());
        assert_eq!(only_bucket(&inventory).probe, Some(probe(42, 1, 1)));
    }

    #[test]
    fn record_check_sets_health_and_leaves_probe() {
        let mut inventory = Inventory::default();
        inventory.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        inventory.record_probe(subnet("192.0.2.10"), probe(1, 1, 1));
        inventory.record_check(subnet("192.0.2.10"), Health::new(0, 2, 99));
        let bucket = only_bucket(&inventory);
        assert_eq!(bucket.health, Some(Health::new(0, 2, 99)));
        assert_eq!(bucket.probe, Some(probe(1, 1, 1)));
    }

    #[test]
    fn record_probe_also_updates_health() {
        let mut inventory = Inventory::default();
        inventory.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        inventory.record_probe(subnet("192.0.2.10"), probe(7, 3, 4));
        assert_eq!(only_bucket(&inventory).health, Some(Health::new(3, 4, 7)));
    }

    #[test]
    fn records_ignore_unknown_subnets() {
        let mut inventory = Inventory::default();
        inventory.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        inventory.record_probe(subnet("198.51.100.2"), probe(1, 1, 1));
        inventory.record_check(subnet("198.51.100.2"), Health::new(1, 1, 1));
        assert_eq!(only_bucket(&inventory).probe, None);
    }

    #[test]
    fn remove_drops_the_given_subnets_only() {
        let mut inventory = Inventory::default();
        inventory.merge(
            by_slash24(vec![
                proxy("192.0.2.10"),
                proxy("198.51.100.2"),
                proxy("203.0.113.9"),
            ]),
            &HashMap::new(),
        );
        inventory.remove(&[subnet("192.0.2.10"), subnet("198.51.100.2")]);
        assert_eq!(inventory.buckets().len(), 1);
        assert_eq!(inventory.buckets()[0].subnet.cidr(), "203.0.113.0/24");
    }

    #[test]
    fn record_countries_refreshes_known_subnets_only() {
        let mut inventory = Inventory::default();
        let mut initial = HashMap::new();
        initial.insert("192.0.2.0/24".to_string(), "FR".to_string());
        initial.insert("198.51.100.0/24".to_string(), "DE".to_string());
        inventory.merge(
            by_slash24(vec![proxy("192.0.2.10"), proxy("198.51.100.2")]),
            &initial,
        );
        let mut refreshed = HashMap::new();
        refreshed.insert("192.0.2.0/24".to_string(), "US".to_string());
        refreshed.insert("203.0.113.0/24".to_string(), "NL".to_string());
        inventory.record_countries(&refreshed);
        let buckets = inventory.buckets();
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].country.as_deref(), Some("US"));
        assert_eq!(buckets[1].country.as_deref(), Some("DE"));
    }

    #[test]
    fn scope_none_returns_all_sorted() {
        let mut inventory = Inventory::default();
        inventory.merge(
            by_slash24(vec![proxy("198.51.100.2"), proxy("192.0.2.10")]),
            &HashMap::new(),
        );
        let cidrs: Vec<_> = inventory
            .scope(None)
            .unwrap()
            .iter()
            .map(|bucket| bucket.subnet.cidr())
            .collect();
        assert_eq!(cidrs, vec!["192.0.2.0/24", "198.51.100.0/24"]);
    }

    #[test]
    fn scope_deduplicates_and_sorts_requested_subnets() {
        let mut inventory = Inventory::default();
        inventory.merge(
            by_slash24(vec![proxy("198.51.100.2"), proxy("192.0.2.10")]),
            &HashMap::new(),
        );
        let cidrs: Vec<_> = inventory
            .scope(Some(vec![
                "198.51.100.0/24".into(),
                "192.0.2.0/24".into(),
                "198.51.100.0/24".into(),
            ]))
            .unwrap()
            .iter()
            .map(|bucket| bucket.subnet.cidr())
            .collect();
        assert_eq!(cidrs, vec!["192.0.2.0/24", "198.51.100.0/24"]);
    }

    #[test]
    fn scope_rejects_empty_malformed_and_missing_subnets() {
        let mut inventory = Inventory::default();
        inventory.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        assert_eq!(
            inventory.scope(Some(Vec::new())).err().as_deref(),
            Some("Select at least one subnet.")
        );
        assert_eq!(
            inventory
                .scope(Some(vec!["192.0.2.10/24".into()]))
                .err()
                .as_deref(),
            Some("Unknown subnet.")
        );
        assert_eq!(
            inventory
                .scope(Some(vec!["198.51.100.0/24".into()]))
                .err()
                .as_deref(),
            Some("Unknown subnet.")
        );
    }
}

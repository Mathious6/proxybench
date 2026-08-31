use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

use crate::parse::ProxyLine;
use crate::split::{Bucket, Subnet};

#[derive(Clone, Debug, PartialEq)]
pub struct StoredMetrics {
    pub ok: usize,
    pub connect_p50: Option<f64>,
    pub connect_p95: Option<f64>,
    pub ttfb_p50: Option<f64>,
    pub ttfb_p95: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredProbe {
    pub at: u64,
    pub metrics: Option<StoredMetrics>,
}

#[derive(Clone)]
pub struct StoredBucket {
    pub subnet: Subnet,
    pub proxies: Vec<ProxyLine>,
    pub country: Option<String>,
    pub last_probe: Option<StoredProbe>,
}

pub struct Merge {
    pub grown: Vec<String>,
}

#[derive(Clone)]
pub struct Session {
    buckets: BTreeMap<Subnet, StoredBucket>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            buckets: BTreeMap::new(),
        }
    }

    pub fn restore(buckets: Vec<StoredBucket>) -> Self {
        Self {
            buckets: buckets
                .into_iter()
                .map(|bucket| (bucket.subnet, bucket))
                .collect(),
        }
    }

    pub fn merge(&mut self, buckets: Vec<Bucket>, countries: &HashMap<String, String>) -> Merge {
        let mut grown = Vec::new();
        for bucket in buckets {
            let cidr = bucket.subnet.cidr();
            match self.buckets.get_mut(&bucket.subnet) {
                Some(existing) => {
                    let before = existing.proxies.len();
                    append_unique(&mut existing.proxies, bucket.proxies);
                    if existing.country.is_none() {
                        existing.country = countries.get(&cidr).cloned();
                    }
                    if existing.proxies.len() > before {
                        existing.last_probe = None;
                        grown.push(cidr);
                    }
                }
                None => {
                    self.buckets.insert(
                        bucket.subnet,
                        StoredBucket {
                            country: countries.get(&cidr).cloned(),
                            subnet: bucket.subnet,
                            proxies: unique_sources(bucket.proxies),
                            last_probe: None,
                        },
                    );
                }
            }
        }
        Merge { grown }
    }

    pub fn remove(&mut self, subnet: Subnet) -> bool {
        self.buckets.remove(&subnet).is_some()
    }

    pub fn record_probe(&mut self, cidr: &str, at: u64, metrics: Option<StoredMetrics>) {
        let Some(subnet) = Subnet::parse_cidr(cidr) else {
            return;
        };
        if let Some(bucket) = self.buckets.get_mut(&subnet) {
            bucket.last_probe = Some(StoredProbe { at, metrics });
        }
    }

    pub fn record_probes(
        &mut self,
        at: u64,
        metrics: &HashMap<String, StoredMetrics>,
        countries: &HashMap<String, String>,
    ) {
        for (cidr, item) in metrics {
            self.record_probe(cidr, at, Some(item.clone()));
        }
        for (cidr, country) in countries {
            let Some(subnet) = Subnet::parse_cidr(cidr) else {
                continue;
            };
            if let Some(bucket) = self.buckets.get_mut(&subnet) {
                bucket.country = Some(country.clone());
            }
        }
    }

    pub fn snapshot(&self) -> Vec<StoredBucket> {
        self.buckets.values().cloned().collect()
    }

    pub fn resolve_scope(&self, cidrs: Option<Vec<String>>) -> Result<Vec<StoredBucket>, String> {
        let Some(cidrs) = cidrs else {
            return Ok(self.snapshot());
        };
        if cidrs.is_empty() {
            return Err("Select at least one subnet.".into());
        }
        let subnets = cidrs
            .into_iter()
            .map(|cidr| Subnet::parse_cidr(&cidr).ok_or_else(|| "Unknown subnet.".to_string()))
            .collect::<Result<BTreeSet<_>, _>>()?;
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

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}

fn unique_sources(proxies: Vec<ProxyLine>) -> Vec<ProxyLine> {
    let mut seen = Vec::new();
    append_unique(&mut seen, proxies);
    seen
}

fn append_unique(existing: &mut Vec<ProxyLine>, incoming: Vec<ProxyLine>) {
    for proxy in incoming {
        if !existing.iter().any(|item| item.source == proxy.source) {
            existing.push(proxy);
        }
    }
}

pub struct SessionStore(pub Mutex<Session>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ProxyLine;
    use crate::split::by_slash24;

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
        let mut item = proxy(host);
        item.source = format!("  {host}:8080:user:pass  ");
        item
    }

    fn sample_metrics(ok: usize) -> StoredMetrics {
        StoredMetrics {
            ok,
            connect_p50: Some(10.0),
            connect_p95: Some(20.0),
            ttfb_p50: Some(30.0),
            ttfb_p95: Some(40.0),
        }
    }

    fn probe(at: u64, ok: usize) -> StoredProbe {
        StoredProbe {
            at,
            metrics: Some(sample_metrics(ok)),
        }
    }

    #[test]
    fn merge_appends_unique_sources_and_keeps_country() {
        let mut session = Session::new();
        let mut countries = HashMap::new();
        countries.insert("192.0.2.0/24".into(), "FR".into());
        session.merge(by_slash24(vec![proxy("192.0.2.10")]), &countries);
        let mut later = HashMap::new();
        later.insert("192.0.2.0/24".into(), "US".into());
        later.insert("198.51.100.0/24".into(), "DE".into());
        let result = session.merge(
            by_slash24(vec![
                proxy("192.0.2.10"),
                proxy("192.0.2.11"),
                spaced("192.0.2.10"),
                proxy("198.51.100.2"),
            ]),
            &later,
        );
        assert_eq!(result.grown, vec!["192.0.2.0/24".to_string()]);
        assert!(session
            .snapshot()
            .iter()
            .any(|bucket| bucket.subnet.cidr() == "198.51.100.0/24"));
        let fr = session
            .snapshot()
            .into_iter()
            .find(|bucket| bucket.subnet.cidr() == "192.0.2.0/24")
            .unwrap();
        assert_eq!(fr.country.as_deref(), Some("FR"));
        assert_eq!(fr.proxies.len(), 3);
        let de = session
            .snapshot()
            .into_iter()
            .find(|bucket| bucket.subnet.cidr() == "198.51.100.0/24")
            .unwrap();
        assert_eq!(de.country.as_deref(), Some("DE"));
    }

    #[test]
    fn merge_fills_country_when_missing() {
        let mut session = Session::new();
        session.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        let mut countries = HashMap::new();
        countries.insert("192.0.2.0/24".into(), "FR".into());
        session.merge(by_slash24(vec![proxy("192.0.2.11")]), &countries);
        let fr = session.snapshot().into_iter().next().unwrap();
        assert_eq!(fr.country.as_deref(), Some("FR"));
        assert_eq!(fr.proxies.len(), 2);
    }

    #[test]
    fn remove_drops_one_subnet() {
        let mut session = Session::new();
        session.merge(
            by_slash24(vec![proxy("192.0.2.10"), proxy("198.51.100.2")]),
            &HashMap::new(),
        );
        assert!(session.remove(Subnet::from_host("192.0.2.10".parse().unwrap())));
        assert!(!session.remove(Subnet::from_host("192.0.2.10".parse().unwrap())));
        assert_eq!(session.snapshot().len(), 1);
        assert_eq!(session.snapshot()[0].subnet.cidr(), "198.51.100.0/24");
    }

    #[test]
    fn snapshot_does_not_depend_on_mutex() {
        let mut session = Session::new();
        session.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        let copy = session.snapshot();
        assert_eq!(copy.len(), 1);
        assert_eq!(copy[0].proxies.len(), 1);
    }

    #[test]
    fn restore_keeps_country_and_sources() {
        let mut session = Session::new();
        let mut countries = HashMap::new();
        countries.insert("192.0.2.0/24".into(), "FR".into());
        session.merge(by_slash24(vec![proxy("192.0.2.10")]), &countries);
        let restored = Session::restore(session.snapshot());
        let bucket = restored.snapshot().into_iter().next().unwrap();
        assert_eq!(bucket.country.as_deref(), Some("FR"));
        assert_eq!(bucket.proxies.len(), 1);
    }

    #[test]
    fn record_probe_sets_metrics_and_merge_clears_when_grown() {
        let mut session = Session::new();
        session.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        session.record_probe("192.0.2.0/24", 42, Some(sample_metrics(1)));
        assert_eq!(session.snapshot()[0].last_probe, Some(probe(42, 1)));
        session.merge(by_slash24(vec![proxy("192.0.2.11")]), &HashMap::new());
        assert_eq!(session.snapshot()[0].last_probe, None);
    }

    #[test]
    fn unchanged_merge_keeps_last_probe() {
        let mut session = Session::new();
        session.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        session.record_probe("192.0.2.0/24", 42, Some(sample_metrics(1)));
        let result = session.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        assert!(result.grown.is_empty());
        assert_eq!(session.snapshot()[0].last_probe, Some(probe(42, 1)));
    }

    #[test]
    fn record_probe_updates_only_matching_cidr() {
        let mut session = Session::new();
        session.merge(
            by_slash24(vec![proxy("192.0.2.10"), proxy("198.51.100.2")]),
            &HashMap::new(),
        );
        let mut metrics = HashMap::new();
        metrics.insert("192.0.2.0/24".into(), sample_metrics(1));
        metrics.insert("203.0.113.0/24".into(), sample_metrics(8));
        session.record_probes(42, &metrics, &HashMap::new());
        let snapshot = session.snapshot();
        let first = snapshot
            .iter()
            .find(|bucket| bucket.subnet.cidr() == "192.0.2.0/24")
            .unwrap();
        let second = snapshot
            .iter()
            .find(|bucket| bucket.subnet.cidr() == "198.51.100.0/24")
            .unwrap();
        assert_eq!(first.last_probe, Some(probe(42, 1)));
        assert_eq!(second.last_probe, None);
    }

    #[test]
    fn record_probes_refreshes_valid_countries_and_keeps_missing_ones() {
        let mut session = Session::new();
        let mut initial = HashMap::new();
        initial.insert("192.0.2.0/24".into(), "FR".into());
        initial.insert("198.51.100.0/24".into(), "DE".into());
        session.merge(
            by_slash24(vec![proxy("192.0.2.10"), proxy("198.51.100.2")]),
            &initial,
        );
        let mut metrics = HashMap::new();
        metrics.insert("192.0.2.0/24".into(), sample_metrics(1));
        metrics.insert("198.51.100.0/24".into(), sample_metrics(1));
        let mut countries = HashMap::new();
        countries.insert("192.0.2.0/24".into(), "US".into());
        session.record_probes(42, &metrics, &countries);
        let snapshot = session.snapshot();
        assert_eq!(snapshot[0].country.as_deref(), Some("US"));
        assert_eq!(snapshot[1].country.as_deref(), Some("DE"));
    }

    #[test]
    fn resolve_scope_returns_all_buckets() {
        let mut session = Session::new();
        session.merge(
            by_slash24(vec![proxy("198.51.100.2"), proxy("192.0.2.10")]),
            &HashMap::new(),
        );
        let buckets = session.resolve_scope(None).unwrap();
        assert_eq!(
            buckets
                .iter()
                .map(|bucket| bucket.subnet.cidr())
                .collect::<Vec<_>>(),
            vec!["192.0.2.0/24", "198.51.100.0/24"]
        );
    }

    #[test]
    fn resolve_scope_deduplicates_and_sorts_subnets() {
        let mut session = Session::new();
        session.merge(
            by_slash24(vec![proxy("198.51.100.2"), proxy("192.0.2.10")]),
            &HashMap::new(),
        );
        let buckets = session
            .resolve_scope(Some(vec![
                "198.51.100.0/24".into(),
                "192.0.2.0/24".into(),
                "198.51.100.0/24".into(),
            ]))
            .unwrap();
        assert_eq!(
            buckets
                .iter()
                .map(|bucket| bucket.subnet.cidr())
                .collect::<Vec<_>>(),
            vec!["192.0.2.0/24", "198.51.100.0/24"]
        );
    }

    #[test]
    fn resolve_scope_rejects_empty_malformed_and_missing_subnets() {
        let mut session = Session::new();
        session.merge(by_slash24(vec![proxy("192.0.2.10")]), &HashMap::new());
        assert!(matches!(
            session.resolve_scope(Some(Vec::new())),
            Err(error) if error == "Select at least one subnet."
        ));
        assert!(matches!(
            session.resolve_scope(Some(vec!["192.0.2.10/24".into()])),
            Err(error) if error == "Unknown subnet."
        ));
        assert!(matches!(
            session.resolve_scope(Some(vec!["198.51.100.0/24".into()])),
            Err(error) if error == "Unknown subnet."
        ));
    }
}

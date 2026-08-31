use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::Ipv4Addr;
use std::thread;
use std::time::{Duration, Instant};

use crate::split::Subnet;

const ENDPOINT: &str = "https://api.country.is/";
const TIMEOUT: Duration = Duration::from_secs(8);
const BATCH_SIZE: usize = 100;
const REQUEST_INTERVAL: Duration = Duration::from_millis(100);

#[derive(serde::Deserialize)]
struct Lookup {
    ip: String,
    country: Option<String>,
}

pub fn lookup(ips: &[Ipv4Addr]) -> HashMap<String, String> {
    lookup_with(ips, fetch)
}

fn lookup_with<F>(ips: &[Ipv4Addr], mut send: F) -> HashMap<String, String>
where
    F: FnMut(&[Ipv4Addr]) -> Option<Vec<Lookup>>,
{
    let ips: Vec<_> = ips
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let requested: HashSet<_> = ips.iter().copied().collect();
    let mut countries = HashMap::new();
    let mut last_request: Option<Instant> = None;

    for batch in ips.chunks(BATCH_SIZE) {
        if let Some(last_request) = last_request {
            let elapsed = last_request.elapsed();
            if elapsed < REQUEST_INTERVAL {
                thread::sleep(REQUEST_INTERVAL - elapsed);
            }
        }
        last_request = Some(Instant::now());
        let Some(rows) = send(batch) else {
            continue;
        };
        for row in rows {
            let Ok(ip) = row.ip.parse::<Ipv4Addr>() else {
                continue;
            };
            if requested.contains(&ip) {
                if let Some(country) = normalize(row.country.as_deref()) {
                    countries.insert(Subnet::from_host(ip).cidr(), country);
                }
            }
        }
    }
    countries
}

fn fetch(ips: &[Ipv4Addr]) -> Option<Vec<Lookup>> {
    let ips: Vec<_> = ips.iter().map(Ipv4Addr::to_string).collect();
    ureq::post(ENDPOINT)
        .timeout(TIMEOUT)
        .send_json(ips)
        .ok()?
        .into_json()
        .ok()
}

fn normalize(value: Option<&str>) -> Option<String> {
    let code = value?;
    if code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        Some(code.to_ascii_uppercase())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips(count: usize) -> Vec<Ipv4Addr> {
        (0..count)
            .map(|index| Ipv4Addr::new(10, (index / 256) as u8, (index % 256) as u8, 1))
            .collect()
    }

    #[test]
    fn lookup_chunks_batches_at_one_hundred() {
        for (count, sizes) in [
            (100, vec![100]),
            (101, vec![100, 1]),
            (250, vec![100, 100, 50]),
        ] {
            let mut sent = Vec::new();
            lookup_with(&ips(count), |batch| {
                sent.push(batch.len());
                Some(Vec::new())
            });
            assert_eq!(sent, sizes);
        }
    }

    #[test]
    fn lookup_deduplicates_ips_and_maps_returned_ips() {
        let first: Ipv4Addr = "192.0.2.10".parse().unwrap();
        let second: Ipv4Addr = "198.51.100.20".parse().unwrap();
        let mut sent = Vec::new();
        let countries = lookup_with(&[first, second, first], |batch| {
            sent.extend_from_slice(batch);
            Some(vec![
                Lookup {
                    ip: second.to_string(),
                    country: Some("de".into()),
                },
                Lookup {
                    ip: first.to_string(),
                    country: Some("fr".into()),
                },
            ])
        });
        assert_eq!(sent.len(), 2);
        assert_eq!(countries.get("192.0.2.0/24"), Some(&"FR".into()));
        assert_eq!(countries.get("198.51.100.0/24"), Some(&"DE".into()));
    }

    #[test]
    fn lookup_ignores_omitted_malformed_and_invalid_rows() {
        let requested: Ipv4Addr = "192.0.2.10".parse().unwrap();
        let countries = lookup_with(&[requested], |_| {
            Some(vec![
                Lookup {
                    ip: "not-an-ip".into(),
                    country: Some("FR".into()),
                },
                Lookup {
                    ip: "198.51.100.20".into(),
                    country: Some("DE".into()),
                },
                Lookup {
                    ip: requested.to_string(),
                    country: Some("FRA".into()),
                },
            ])
        });
        assert!(countries.is_empty());
    }

    #[test]
    fn normalize_uppercases_iso_alpha2() {
        assert_eq!(normalize(Some("fr")), Some("FR".into()));
        assert_eq!(normalize(Some("US")), Some("US".into()));
    }

    #[test]
    fn normalize_rejects_empty_and_non_iso() {
        assert_eq!(normalize(None), None);
        assert_eq!(normalize(Some("")), None);
        assert_eq!(normalize(Some("FRA")), None);
        assert_eq!(normalize(Some("12")), None);
    }
}

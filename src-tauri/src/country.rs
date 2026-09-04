use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::net::Ipv4Addr;
use std::thread;
use std::time::{Duration, Instant};

use crate::dns;
use crate::split::Subnet;

const ENDPOINT: &str = "https://api.country.is/";
const TIMEOUT: Duration = Duration::from_secs(8);
const BATCH_SIZE: usize = 100;
const REQUEST_INTERVAL: Duration = Duration::from_millis(100);
const RETRY_DELAY: Duration = Duration::from_millis(500);
const ROUND_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];
const ATTEMPTS: usize = 3;
const ROUNDS: usize = 4;

#[derive(Debug, serde::Deserialize)]
struct Lookup {
    ip: String,
    country: Option<String>,
}

#[derive(Debug)]
pub enum LookupError {
    Http,
    Decode,
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LookupError::Http => write!(f, "country lookup request failed"),
            LookupError::Decode => write!(f, "country lookup response unreadable"),
        }
    }
}

pub fn lookup(ips: &[Ipv4Addr]) -> HashMap<String, String> {
    let agent = fetch_agent();
    lookup_with(ips, |batch| fetch(&agent, batch))
}

fn fetch_agent() -> ureq::Agent {
    ureq::AgentBuilder::new().resolver(dns::resolve).build()
}

fn lookup_with<F>(ips: &[Ipv4Addr], mut send: F) -> HashMap<String, String>
where
    F: FnMut(&[Ipv4Addr]) -> Result<Vec<Lookup>, LookupError>,
{
    let unique: Vec<_> = ips
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let requested: HashSet<_> = unique.iter().copied().collect();
    let mut countries = HashMap::new();
    let mut batches: Vec<&[Ipv4Addr]> = unique.chunks(BATCH_SIZE).collect();
    let mut last_request: Option<Instant> = None;
    for round in 0..ROUNDS {
        let mut unresolved: Vec<&[Ipv4Addr]> = Vec::new();
        for batch in batches {
            match send_with_retries(batch, &mut send, &mut last_request) {
                Some(rows) => record(&mut countries, &requested, rows),
                None => unresolved.push(batch),
            }
        }
        if unresolved.is_empty() {
            break;
        }
        match ROUND_DELAYS.get(round) {
            Some(delay) => thread::sleep(*delay),
            None => break,
        }
        batches = unresolved;
    }
    countries
}

fn send_with_retries<F>(
    batch: &[Ipv4Addr],
    send: &mut F,
    last_request: &mut Option<Instant>,
) -> Option<Vec<Lookup>>
where
    F: FnMut(&[Ipv4Addr]) -> Result<Vec<Lookup>, LookupError>,
{
    for attempt in 0..ATTEMPTS {
        wait_for_request(*last_request);
        *last_request = Some(Instant::now());
        match send(batch) {
            Ok(rows) => return Some(rows),
            Err(_) if attempt + 1 < ATTEMPTS => thread::sleep(RETRY_DELAY),
            Err(_) => {}
        }
    }
    None
}

fn record(
    countries: &mut HashMap<String, String>,
    requested: &HashSet<Ipv4Addr>,
    rows: Vec<Lookup>,
) {
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

fn wait_for_request(last_request: Option<Instant>) {
    if let Some(last_request) = last_request {
        let elapsed = last_request.elapsed();
        if elapsed < REQUEST_INTERVAL {
            thread::sleep(REQUEST_INTERVAL - elapsed);
        }
    }
}

fn fetch(agent: &ureq::Agent, ips: &[Ipv4Addr]) -> Result<Vec<Lookup>, LookupError> {
    let ips: Vec<_> = ips.iter().map(Ipv4Addr::to_string).collect();
    let response = agent
        .post(ENDPOINT)
        .timeout(TIMEOUT)
        .send_json(ips)
        .map_err(|_| LookupError::Http)?;
    response.into_json().map_err(|_| LookupError::Decode)
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
                Ok(Vec::new())
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
            Ok(vec![
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
            Ok(vec![
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
    fn lookup_retries_failed_batches() {
        let requested: Ipv4Addr = "151.242.94.0".parse().unwrap();
        let mut attempts = 0;
        let countries = lookup_with(&[requested], |_| {
            attempts += 1;
            if attempts == 1 {
                Err(LookupError::Http)
            } else {
                Ok(vec![Lookup {
                    ip: requested.to_string(),
                    country: Some("DE".into()),
                }])
            }
        });
        assert_eq!(attempts, 2);
        assert_eq!(countries.get("151.242.94.0/24"), Some(&"DE".into()));
    }

    #[test]
    fn lookup_retries_unresolved_batches_in_a_later_round() {
        let requested: Ipv4Addr = "192.0.2.10".parse().unwrap();
        let mut calls = 0;
        let countries = lookup_with(&[requested], |_| {
            calls += 1;
            if calls <= ATTEMPTS {
                Err(LookupError::Http)
            } else {
                Ok(vec![Lookup {
                    ip: requested.to_string(),
                    country: Some("DE".into()),
                }])
            }
        });
        assert_eq!(calls, ATTEMPTS + 1);
        assert_eq!(countries.get("192.0.2.0/24"), Some(&"DE".into()));
    }

    #[test]
    fn lookup_resends_only_unresolved_batches_in_a_later_round() {
        let requested: Ipv4Addr = "192.0.2.0".parse().unwrap();
        let other: Ipv4Addr = "198.51.100.20".parse().unwrap();
        let mut all: Vec<Ipv4Addr> = (0..100)
            .map(|offset| Ipv4Addr::new(192, 0, 2, offset))
            .collect();
        all.push(other);
        let mut bulk_calls = 0;
        let mut single_calls = 0;
        let countries = lookup_with(&all, |batch| {
            if batch.len() > 1 {
                bulk_calls += 1;
                if bulk_calls <= ATTEMPTS {
                    return Err(LookupError::Http);
                }
            } else {
                single_calls += 1;
            }
            Ok(vec![Lookup {
                ip: batch[0].to_string(),
                country: Some("DE".into()),
            }])
        });
        assert_eq!(bulk_calls, ATTEMPTS + 1);
        assert_eq!(single_calls, 1);
        assert_eq!(
            countries.get(&Subnet::from_host(requested).cidr()),
            Some(&"DE".into())
        );
        assert_eq!(countries.get("198.51.100.0/24"), Some(&"DE".into()));
    }

    #[test]
    fn lookup_gives_up_after_all_rounds() {
        let requested: Ipv4Addr = "192.0.2.10".parse().unwrap();
        let mut calls = 0;
        let countries = lookup_with(&[requested], |_| {
            calls += 1;
            Err::<Vec<Lookup>, LookupError>(LookupError::Http)
        });
        assert_eq!(calls, ATTEMPTS * ROUNDS);
        assert!(countries.is_empty());
    }

    #[test]
    #[ignore = "hits the live api.country.is service"]
    fn lookup_resolves_live_subnets() {
        let ips: Vec<Ipv4Addr> = ["140.82.121.3", "140.82.121.4", "140.82.121.5"]
            .iter()
            .map(|value| value.parse().unwrap())
            .collect();
        let countries = lookup(&ips);
        assert!(countries.contains_key("140.82.121.0/24"));
    }

    #[test]
    fn lookup_decodes_bulk_response() {
        let rows: Vec<Lookup> =
            serde_json::from_str(r#"[{"ip":"151.242.94.0","country":"DE"}]"#).unwrap();
        assert_eq!(rows[0].ip, "151.242.94.0");
        assert_eq!(rows[0].country.as_deref(), Some("DE"));
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

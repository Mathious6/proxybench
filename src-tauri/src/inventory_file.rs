use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::health::Health;
use crate::inventory::Bucket;
use crate::measurement::Probe;
use crate::proxy::{self, ProxyLine};
use crate::secure_file;
use crate::subnet::Subnet;

#[derive(Default, Serialize, Deserialize)]
struct Disk {
    buckets: Vec<DiskBucket>,
}

#[derive(Serialize, Deserialize)]
struct DiskBucket {
    cidr: String,
    country: Option<String>,
    proxies: Vec<String>,
    #[serde(default)]
    last_run_at: Option<u64>,
    #[serde(default)]
    metrics: Option<DiskMetrics>,
    #[serde(default)]
    health: Option<DiskHealth>,
}

#[derive(Serialize, Deserialize)]
struct DiskMetrics {
    ok: usize,
    #[serde(default)]
    tested: Option<usize>,
    connect_p50: Option<f64>,
    connect_p95: Option<f64>,
    ttfb_p50: Option<f64>,
    ttfb_p95: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct DiskHealth {
    ok: usize,
    tested: usize,
    at: u64,
}

pub struct InventoryFile {
    path: PathBuf,
}

impl InventoryFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Vec<Bucket>, String> {
        match fs::read(&self.path) {
            Ok(bytes) => decode(&bytes),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn save(&self, buckets: &[Bucket]) -> Result<(), String> {
        let disk = Disk {
            buckets: buckets.iter().map(encode_bucket).collect(),
        };
        let bytes = serde_json::to_vec_pretty(&disk).map_err(json_error)?;
        secure_file::write_atomic(&self.path, &bytes)
    }
}

fn encode_bucket(bucket: &Bucket) -> DiskBucket {
    DiskBucket {
        cidr: bucket.subnet.cidr(),
        country: bucket.country.clone(),
        proxies: bucket
            .proxies
            .iter()
            .map(|proxy| proxy.source.clone())
            .collect(),
        last_run_at: bucket.probe.as_ref().map(|probe| probe.at),
        metrics: bucket.probe.as_ref().map(|probe| DiskMetrics {
            ok: probe.ok,
            tested: Some(probe.tested),
            connect_p50: probe.connect_p50,
            connect_p95: probe.connect_p95,
            ttfb_p50: probe.ttfb_p50,
            ttfb_p95: probe.ttfb_p95,
        }),
        health: bucket.health.map(|health| DiskHealth {
            ok: health.ok,
            tested: health.tested,
            at: health.at,
        }),
    }
}

fn decode(bytes: &[u8]) -> Result<Vec<Bucket>, String> {
    let disk: Disk = serde_json::from_slice(bytes).map_err(json_error)?;
    let mut merged: BTreeMap<Subnet, Bucket> = BTreeMap::new();
    let mut stale: HashSet<Subnet> = HashSet::new();
    for stored in disk.buckets {
        let Some(incoming) = decode_bucket(stored) else {
            continue;
        };
        let Some(existing) = merged.get_mut(&incoming.subnet) else {
            merged.insert(incoming.subnet, incoming);
            continue;
        };
        if incoming.country.is_some() {
            existing.country = incoming.country.clone();
        }
        let grew = incoming.proxies.iter().any(|proxy| {
            !existing
                .proxies
                .iter()
                .any(|known| known.source == proxy.source)
        });
        existing.absorb(incoming.proxies.clone());
        if grew {
            stale.insert(existing.subnet);
            existing.clear_results();
        } else if !stale.contains(&existing.subnet) {
            keep_latest(existing, incoming);
        }
    }
    Ok(merged.into_values().collect())
}

fn decode_bucket(stored: DiskBucket) -> Option<Bucket> {
    let subnet = Subnet::parse_cidr(&stored.cidr)?;
    let proxies: Vec<ProxyLine> = stored
        .proxies
        .iter()
        .filter_map(|line| parse_source(line, subnet))
        .collect();
    let probe = match (stored.last_run_at, stored.metrics) {
        (Some(at), Some(metrics)) => Some(Probe {
            at,
            ok: metrics.ok,
            tested: metrics.tested.unwrap_or(proxies.len()),
            connect_p50: metrics.connect_p50,
            connect_p95: metrics.connect_p95,
            ttfb_p50: metrics.ttfb_p50,
            ttfb_p95: metrics.ttfb_p95,
        }),
        _ => None,
    };
    let health = match stored.health {
        Some(health) => Some(Health::new(health.ok, health.tested, health.at)),
        None => probe
            .as_ref()
            .map(|probe| Health::new(probe.ok, probe.tested, probe.at)),
    };
    let mut bucket = Bucket::new(subnet, proxies, stored.country);
    bucket.probe = probe;
    bucket.health = health;
    Some(bucket)
}

fn keep_latest(existing: &mut Bucket, incoming: Bucket) {
    let newer_probe = match (&existing.probe, &incoming.probe) {
        (Some(current), Some(next)) => next.at > current.at,
        (None, Some(_)) => true,
        _ => false,
    };
    if newer_probe {
        existing.probe = incoming.probe;
    }
    let newer_health = match (existing.health, incoming.health) {
        (Some(current), Some(next)) => next.at > current.at,
        (None, Some(_)) => true,
        _ => false,
    };
    if newer_health {
        existing.health = incoming.health;
    }
}

fn parse_source(line: &str, subnet: Subnet) -> Option<ProxyLine> {
    let (proxies, skipped) = proxy::parse_text(&format!("{line}\n"));
    if skipped > 0 || proxies.len() != 1 {
        return None;
    }
    let parsed = proxies.into_iter().next()?;
    if Subnet::from_host(parsed.host) != subnet {
        return None;
    }
    Some(parsed)
}

fn json_error(err: serde_json::Error) -> String {
    format!(
        "Could not read inventory (line {} column {})",
        err.line(),
        err.column()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_file() -> (PathBuf, InventoryFile) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "proxybench-inventory-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let file = InventoryFile::new(dir.join("proxies.json"));
        (dir, file)
    }

    fn write_json(dir: &Path, json: &str) {
        fs::write(dir.join("proxies.json"), json).unwrap();
    }

    fn bucket(host: &str, country: Option<&str>, sources: &[&str]) -> Bucket {
        let subnet = Subnet::from_host(host.parse().unwrap());
        let proxies = sources
            .iter()
            .filter_map(|source| parse_source(source, subnet))
            .collect();
        Bucket::new(subnet, proxies, country.map(str::to_string))
    }

    fn sample_probe(ok: usize, tested: usize) -> Probe {
        Probe {
            at: 1_700_000_000_000,
            ok,
            tested,
            connect_p50: Some(12.5),
            connect_p95: Some(40.0),
            ttfb_p50: Some(80.0),
            ttfb_p95: Some(120.0),
        }
    }

    #[test]
    fn save_then_load_roundtrips_verbatim_sources() {
        let (dir, file) = temp_file();
        file.save(&[bucket(
            "192.0.2.10",
            Some("FR"),
            &["  192.0.2.10:8080:user:p:ss  "],
        )])
        .unwrap();
        let loaded = file.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].country.as_deref(), Some("FR"));
        assert_eq!(loaded[0].proxies[0].source, "  192.0.2.10:8080:user:p:ss  ");
        assert_eq!(loaded[0].proxies[0].password, "p:ss");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrips_probe_and_health() {
        let (dir, file) = temp_file();
        let mut stored = bucket("192.0.2.10", Some("FR"), &["192.0.2.10:8080:user:pass"]);
        stored.probe = Some(sample_probe(3, 4));
        stored.health = Some(Health::new(0, 2, 1_800_000_000_000));
        file.save(&[stored]).unwrap();
        let loaded = file.load().unwrap();
        assert_eq!(loaded[0].probe, Some(sample_probe(3, 4)));
        assert_eq!(loaded[0].health, Some(Health::new(0, 2, 1_800_000_000_000)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrips_all_failed_probe() {
        let (dir, file) = temp_file();
        let mut stored = bucket("192.0.2.10", None, &["192.0.2.10:8080:user:pass"]);
        stored.probe = Some(Probe {
            at: 5,
            ok: 0,
            tested: 1,
            connect_p50: None,
            connect_p95: None,
            ttfb_p50: None,
            ttfb_p95: None,
        });
        file.save(&[stored]).unwrap();
        let loaded = file.load().unwrap();
        let probe = loaded[0].probe.clone().unwrap();
        assert_eq!((probe.ok, probe.tested), (0, 1));
        assert_eq!(probe.connect_p50, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_accepts_earlier_files_without_health_or_tested() {
        let (dir, file) = temp_file();
        write_json(
            &dir,
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass","192.0.2.11:8080:user:pass"],"last_run_at":1700000000000,"metrics":{"ok":2,"connect_p50":1.0,"connect_p95":2.0,"ttfb_p50":3.0,"ttfb_p95":4.0}}]}"#,
        );
        let loaded = file.load().unwrap();
        let probe = loaded[0].probe.clone().unwrap();
        assert_eq!(
            (probe.ok, probe.tested, probe.at),
            (2, 2, 1_700_000_000_000)
        );
        assert_eq!(loaded[0].health, Some(Health::new(2, 2, 1_700_000_000_000)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_accepts_files_with_only_cidr_country_and_proxies() {
        let (dir, file) = temp_file();
        write_json(
            &dir,
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"]}]}"#,
        );
        let loaded = file.load().unwrap();
        assert_eq!(loaded[0].probe, None);
        assert_eq!(loaded[0].health, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_ignores_unknown_fields_and_orphan_metrics() {
        let (dir, file) = temp_file();
        write_json(
            &dir,
            r#"{"extra":1,"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"retired":true,"metrics":{"ok":3,"connect_p50":1.0,"connect_p95":2.0,"ttfb_p50":3.0,"ttfb_p95":4.0}}]}"#,
        );
        let loaded = file.load().unwrap();
        assert_eq!(loaded[0].probe, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_clears_results_when_duplicate_cidr_grows() {
        let (dir, file) = temp_file();
        write_json(
            &dir,
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":100,"metrics":{"ok":1,"connect_p50":1.0,"connect_p95":2.0,"ttfb_p50":3.0,"ttfb_p95":4.0}},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.11:8080:user:pass"]},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":300,"metrics":{"ok":2,"connect_p50":5.0,"connect_p95":6.0,"ttfb_p50":7.0,"ttfb_p95":8.0}}]}"#,
        );
        let loaded = file.load().unwrap();
        assert_eq!(loaded[0].proxies.len(), 2);
        assert_eq!(loaded[0].probe, None);
        assert_eq!(loaded[0].health, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_keeps_latest_results_when_duplicate_cidr_is_unchanged() {
        let (dir, file) = temp_file();
        write_json(
            &dir,
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":100,"metrics":{"ok":1,"connect_p50":1.0,"connect_p95":2.0,"ttfb_p50":3.0,"ttfb_p95":4.0}},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":200,"metrics":{"ok":2,"connect_p50":5.0,"connect_p95":6.0,"ttfb_p50":7.0,"ttfb_p95":8.0}}]}"#,
        );
        let loaded = file.load().unwrap();
        assert_eq!(loaded[0].proxies.len(), 1);
        let probe = loaded[0].probe.clone().unwrap();
        assert_eq!((probe.at, probe.ok, probe.connect_p50), (200, 2, Some(5.0)));
        assert_eq!(loaded[0].health.unwrap().at, 200);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let (dir, file) = temp_file();
        assert!(file.load().unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_invalid_json_without_source_text() {
        let (dir, file) = temp_file();
        write_json(
            &dir,
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":null,"proxies":"192.0.2.10:8080:user:secret"}]}"#,
        );
        let err = match file.load() {
            Ok(_) => panic!("expected invalid json"),
            Err(err) => err,
        };
        assert!(!err.contains("secret"));
        assert!(!err.contains("user"));
        assert!(err.contains("line"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_skips_corrupt_cidr_and_foreign_lines() {
        let (dir, file) = temp_file();
        write_json(
            &dir,
            r#"{"buckets":[{"cidr":"not-a-cidr","country":null,"proxies":["bad"]},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass","198.51.100.2:8080:user:pass"]}]}"#,
        );
        let loaded = file.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].proxies.len(), 1);
        assert_eq!(
            loaded[0].proxies[0].host,
            "192.0.2.10".parse::<std::net::Ipv4Addr>().unwrap()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

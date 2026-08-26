use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::parse::{self, ProxyLine};
use crate::session::{self, StoredBucket, StoredMetrics, StoredProbe};
use crate::split::{Bucket, Subnet};

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Disk {
    buckets: Vec<DiskBucket>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct DiskBucket {
    cidr: String,
    country: Option<String>,
    proxies: Vec<String>,
    #[serde(default)]
    last_run_at: Option<u64>,
    #[serde(default)]
    metrics: Option<DiskMetrics>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct DiskMetrics {
    ok: usize,
    connect_p50: Option<f64>,
    connect_p95: Option<f64>,
    ttfb_p50: Option<f64>,
    ttfb_p95: Option<f64>,
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Vec<StoredBucket>, String> {
        match fs::read(&self.path) {
            Ok(bytes) => decode(&bytes),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn save(&self, buckets: &[StoredBucket]) -> Result<(), String> {
        let disk = Disk {
            buckets: buckets.iter().map(to_disk).collect(),
        };
        let bytes = serde_json::to_vec_pretty(&disk).map_err(json_error)?;
        write_atomic(&self.path, &bytes)
    }
}

fn to_disk(bucket: &StoredBucket) -> DiskBucket {
    DiskBucket {
        cidr: bucket.subnet.cidr(),
        country: bucket.country.clone(),
        proxies: bucket
            .proxies
            .iter()
            .map(|proxy| proxy.source.clone())
            .collect(),
        last_run_at: bucket.last_probe.as_ref().map(|probe| probe.at),
        metrics: bucket
            .last_probe
            .as_ref()
            .and_then(|probe| probe.metrics.as_ref())
            .map(to_disk_metrics),
    }
}

fn to_disk_metrics(metrics: &StoredMetrics) -> DiskMetrics {
    DiskMetrics {
        ok: metrics.ok,
        connect_p50: metrics.connect_p50,
        connect_p95: metrics.connect_p95,
        ttfb_p50: metrics.ttfb_p50,
        ttfb_p95: metrics.ttfb_p95,
    }
}

fn from_disk_metrics(metrics: DiskMetrics) -> StoredMetrics {
    StoredMetrics {
        ok: metrics.ok,
        connect_p50: metrics.connect_p50,
        connect_p95: metrics.connect_p95,
        ttfb_p50: metrics.ttfb_p50,
        ttfb_p95: metrics.ttfb_p95,
    }
}

fn decode(bytes: &[u8]) -> Result<Vec<StoredBucket>, String> {
    let disk: Disk = serde_json::from_slice(bytes).map_err(json_error)?;
    let mut countries = HashMap::new();
    let mut probes: HashMap<String, Option<StoredProbe>> = HashMap::new();
    let mut stale = HashSet::new();
    let mut groups: BTreeMap<Subnet, Vec<ProxyLine>> = BTreeMap::new();
    for bucket in disk.buckets {
        let Some(subnet) = Subnet::parse_cidr(&bucket.cidr) else {
            continue;
        };
        let cidr = subnet.cidr();
        if let Some(code) = bucket.country {
            countries.insert(cidr.clone(), code);
        }
        let incoming: Vec<ProxyLine> = bucket
            .proxies
            .iter()
            .filter_map(|line| parse_source(line, subnet))
            .collect();
        let grown = match groups.get(&subnet) {
            Some(existing) => incoming
                .iter()
                .any(|proxy| !existing.iter().any(|item| item.source == proxy.source)),
            None => false,
        };
        let probe = bucket.last_run_at.map(|at| StoredProbe {
            at,
            metrics: bucket.metrics.map(from_disk_metrics),
        });
        if grown {
            stale.insert(cidr.clone());
            probes.insert(cidr, None);
        } else if !stale.contains(&cidr) {
            match probes.get_mut(&cidr) {
                Some(stored) => {
                    if let Some(next) = probe {
                        if stored
                            .as_ref()
                            .map(|current| next.at > current.at)
                            .unwrap_or(true)
                        {
                            *stored = Some(next);
                        }
                    }
                }
                None => {
                    probes.insert(cidr, probe);
                }
            }
        }
        let entry = groups.entry(subnet).or_default();
        for proxy in incoming {
            if !entry.iter().any(|item| item.source == proxy.source) {
                entry.push(proxy);
            }
        }
    }
    let buckets = groups
        .into_iter()
        .map(|(subnet, proxies)| Bucket { subnet, proxies })
        .collect();
    let mut session = session::Session::new();
    session.merge(buckets, &countries);
    for (cidr, probe) in probes {
        if let Some(probe) = probe {
            session.record_probe(&cidr, probe.at, probe.metrics);
        }
    }
    Ok(session.snapshot())
}

fn parse_source(line: &str, subnet: Subnet) -> Option<ProxyLine> {
    let (items, skipped) = parse::parse_text(&format!("{line}\n"));
    if skipped > 0 || items.len() != 1 {
        return None;
    }
    let proxy = items.into_iter().next()?;
    if Subnet::from_host(proxy.host) != subnet {
        return None;
    }
    Some(proxy)
}

fn json_error(err: serde_json::Error) -> String {
    format!(
        "Could not read inventory (line {} column {})",
        err.line(),
        err.column()
    )
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    write_mode(&tmp, bytes).map_err(|err| err.to_string())?;
    fs::rename(&tmp, path).map_err(|err| {
        let _ = fs::remove_file(&tmp);
        err.to_string()
    })
}

fn write_mode(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(bytes)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_store() -> (PathBuf, Store) {
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
        let path = dir.join("proxies.json");
        (dir, Store::new(path))
    }

    fn bucket(host: &str, country: Option<&str>, sources: &[&str]) -> StoredBucket {
        StoredBucket {
            subnet: Subnet::from_host(host.parse().unwrap()),
            country: country.map(str::to_string),
            proxies: sources
                .iter()
                .filter_map(|source| parse_source(source, Subnet::from_host(host.parse().unwrap())))
                .collect(),
            last_probe: None,
        }
    }

    fn sample_metrics(ok: usize) -> StoredMetrics {
        StoredMetrics {
            ok,
            connect_p50: Some(12.5),
            connect_p95: Some(40.0),
            ttfb_p50: Some(80.0),
            ttfb_p95: Some(120.0),
        }
    }

    #[test]
    fn save_then_load_roundtrips_verbatim_sources() {
        let (dir, store) = temp_store();
        store
            .save(&[bucket(
                "192.0.2.10",
                Some("FR"),
                &["  192.0.2.10:8080:user:p:ss  "],
            )])
            .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].country.as_deref(), Some("FR"));
        assert_eq!(loaded[0].proxies[0].source, "  192.0.2.10:8080:user:p:ss  ");
        assert_eq!(loaded[0].proxies[0].password, "p:ss");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrips_last_run_at() {
        let (dir, store) = temp_store();
        let mut item = bucket("192.0.2.10", Some("FR"), &["192.0.2.10:8080:user:pass"]);
        item.last_probe = Some(StoredProbe {
            at: 1_700_000_000_000,
            metrics: None,
        });
        store.save(&[item]).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(
            loaded[0].last_probe,
            Some(StoredProbe {
                at: 1_700_000_000_000,
                metrics: None,
            })
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrips_metrics() {
        let (dir, store) = temp_store();
        let mut item = bucket("192.0.2.10", Some("FR"), &["192.0.2.10:8080:user:pass"]);
        item.last_probe = Some(StoredProbe {
            at: 1_700_000_000_000,
            metrics: Some(sample_metrics(4)),
        });
        store.save(&[item]).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(
            loaded[0].last_probe,
            Some(StoredProbe {
                at: 1_700_000_000_000,
                metrics: Some(sample_metrics(4)),
            })
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrips_all_failed_metrics() {
        let (dir, store) = temp_store();
        let mut item = bucket("192.0.2.10", Some("FR"), &["192.0.2.10:8080:user:pass"]);
        item.last_probe = Some(StoredProbe {
            at: 1_700_000_000_000,
            metrics: Some(StoredMetrics {
                ok: 0,
                connect_p50: None,
                connect_p95: None,
                ttfb_p50: None,
                ttfb_p95: None,
            }),
        });
        store.save(&[item]).unwrap();
        let loaded = store.load().unwrap();
        let probe = loaded[0].last_probe.clone().unwrap();
        assert_eq!(probe.at, 1_700_000_000_000);
        let metrics = probe.metrics.unwrap();
        assert_eq!(metrics.ok, 0);
        assert_eq!(metrics.connect_p50, None);
        assert_eq!(metrics.connect_p95, None);
        assert_eq!(metrics.ttfb_p50, None);
        assert_eq!(metrics.ttfb_p95, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_accepts_legacy_buckets_without_last_run_at() {
        let (dir, store) = temp_store();
        fs::write(
            dir.join("proxies.json"),
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"]}]}"#,
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded[0].last_probe, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_accepts_legacy_last_run_at_without_metrics() {
        let (dir, store) = temp_store();
        fs::write(
            dir.join("proxies.json"),
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":1700000000000}]}"#,
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(
            loaded[0].last_probe,
            Some(StoredProbe {
                at: 1_700_000_000_000,
                metrics: None,
            })
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_ignores_orphan_metrics_without_last_run_at() {
        let (dir, store) = temp_store();
        fs::write(
            dir.join("proxies.json"),
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"metrics":{"ok":3,"connect_p50":1.0,"connect_p95":2.0,"ttfb_p50":3.0,"ttfb_p95":4.0}}]}"#,
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded[0].last_probe, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_clears_probe_when_duplicate_cidr_grows() {
        let (dir, store) = temp_store();
        fs::write(
            dir.join("proxies.json"),
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":1700000000000,"metrics":{"ok":1,"connect_p50":10.0,"connect_p95":20.0,"ttfb_p50":30.0,"ttfb_p95":40.0}},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.11:8080:user:pass"]}]}"#,
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded[0].proxies.len(), 2);
        assert_eq!(loaded[0].last_probe, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_keeps_probe_cleared_after_later_duplicate_of_grown_cidr() {
        let (dir, store) = temp_store();
        fs::write(
            dir.join("proxies.json"),
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":100,"metrics":{"ok":1,"connect_p50":1.0,"connect_p95":2.0,"ttfb_p50":3.0,"ttfb_p95":4.0}},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.11:8080:user:pass"]},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":300,"metrics":{"ok":2,"connect_p50":5.0,"connect_p95":6.0,"ttfb_p50":7.0,"ttfb_p95":8.0}}]}"#,
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded[0].proxies.len(), 2);
        assert_eq!(loaded[0].last_probe, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_keeps_latest_probe_when_duplicate_cidr_is_unchanged() {
        let (dir, store) = temp_store();
        fs::write(
            dir.join("proxies.json"),
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":100,"metrics":{"ok":1,"connect_p50":1.0,"connect_p95":2.0,"ttfb_p50":3.0,"ttfb_p95":4.0}},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass"],"last_run_at":200,"metrics":{"ok":2,"connect_p50":5.0,"connect_p95":6.0,"ttfb_p50":7.0,"ttfb_p95":8.0}}]}"#,
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded[0].proxies.len(), 1);
        assert_eq!(
            loaded[0].last_probe,
            Some(StoredProbe {
                at: 200,
                metrics: Some(StoredMetrics {
                    ok: 2,
                    connect_p50: Some(5.0),
                    connect_p95: Some(6.0),
                    ttfb_p50: Some(7.0),
                    ttfb_p95: Some(8.0),
                }),
            })
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let (dir, store) = temp_store();
        assert!(store.load().unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_invalid_json_without_source_text() {
        let (dir, store) = temp_store();
        fs::write(
            dir.join("proxies.json"),
            r#"{"buckets":[{"cidr":"192.0.2.0/24","country":null,"proxies":"192.0.2.10:8080:user:secret"}]}"#,
        )
        .unwrap();
        let err = match store.load() {
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
        let (dir, store) = temp_store();
        fs::write(
            dir.join("proxies.json"),
            r#"{"buckets":[{"cidr":"not-a-cidr","country":null,"proxies":["bad"]},{"cidr":"192.0.2.0/24","country":"FR","proxies":["192.0.2.10:8080:user:pass","198.51.100.2:8080:user:pass"]}]}"#,
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].proxies.len(), 1);
        assert_eq!(
            loaded[0].proxies[0].host,
            "192.0.2.10".parse::<std::net::Ipv4Addr>().unwrap()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

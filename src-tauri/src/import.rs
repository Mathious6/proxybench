use std::fs;
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::country;
use crate::inventory;
use crate::parse;
use crate::session::{SessionStore, StoredBucket};
use crate::split;
use crate::tags::{Store, Tag};

const TAGS_FILE: &str = "tags.json";
const PROXIES_FILE: &str = "proxies.json";

pub struct TagStore(pub Mutex<Store>);
pub struct InventoryStore(pub inventory::Store);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubnetRow {
    pub cidr: String,
    pub country: Option<String>,
    pub quantity: usize,
    pub tags: Vec<String>,
    pub ok: Option<usize>,
    pub connect_p50: Option<f64>,
    pub connect_p95: Option<f64>,
    pub ttfb_p50: Option<f64>,
    pub ttfb_p95: Option<f64>,
    pub last_run_at: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub rows: Vec<SubnetRow>,
    pub skipped: usize,
    pub grown: Vec<String>,
}

pub fn open_store(app: &AppHandle) -> Result<Store, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    Store::load(dir.join(TAGS_FILE))
}

pub fn open_inventory(app: &AppHandle) -> Result<inventory::Store, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    Ok(inventory::Store::new(dir.join(PROXIES_FILE)))
}

pub fn rows_from(buckets: &[StoredBucket], tags: &Store) -> Vec<SubnetRow> {
    buckets
        .iter()
        .map(|bucket| {
            let cidr = bucket.subnet.cidr();
            let metrics = bucket
                .last_probe
                .as_ref()
                .and_then(|probe| probe.metrics.as_ref());
            SubnetRow {
                country: bucket.country.clone(),
                quantity: bucket.proxies.len(),
                tags: tags
                    .get(&cidr)
                    .into_iter()
                    .map(|tag| tag.as_str().to_string())
                    .collect(),
                cidr,
                ok: metrics.map(|item| item.ok),
                connect_p50: metrics.and_then(|item| item.connect_p50),
                connect_p95: metrics.and_then(|item| item.connect_p95),
                ttfb_p50: metrics.and_then(|item| item.ttfb_p50),
                ttfb_p95: metrics.and_then(|item| item.ttfb_p95),
                last_run_at: bucket.last_probe.as_ref().map(|probe| probe.at),
            }
        })
        .collect()
}

#[tauri::command]
pub async fn import_paths(
    paths: Vec<String>,
    store: State<'_, TagStore>,
    session: State<'_, SessionStore>,
    inventory: State<'_, InventoryStore>,
) -> Result<ImportResult, String> {
    let mut text = String::new();
    for path in paths {
        append_path(Path::new(&path), &mut text)?;
    }
    let (proxies, skipped) = parse::parse_text(&text);
    let incoming = split::by_slash24(proxies);
    let known = {
        let session = session.0.lock().map_err(|err| err.to_string())?;
        session.snapshot()
    };
    let samples: Vec<_> = incoming
        .iter()
        .filter(|bucket| needs_country(&known, bucket.subnet.cidr()))
        .filter_map(|bucket| bucket.proxies.first().map(|proxy| proxy.host))
        .collect();
    let countries = tauri::async_runtime::spawn_blocking(move || country::lookup(&samples))
        .await
        .map_err(|err| err.to_string())?;
    let (snapshot, grown) = {
        let mut session = session.0.lock().map_err(|err| err.to_string())?;
        let mut candidate = session.clone();
        let merge = candidate.merge(incoming, &countries);
        let snapshot = candidate.snapshot();
        inventory.0.save(&snapshot)?;
        *session = candidate;
        (snapshot, merge.grown)
    };
    let tags = store.0.lock().map_err(|err| err.to_string())?;
    Ok(ImportResult {
        rows: rows_from(&snapshot, &tags),
        skipped,
        grown,
    })
}

fn needs_country(known: &[StoredBucket], cidr: String) -> bool {
    known
        .iter()
        .find(|bucket| bucket.subnet.cidr() == cidr)
        .map(|bucket| bucket.country.is_none())
        .unwrap_or(true)
}

#[tauri::command]
pub fn set_tags(
    cidr: String,
    tags: Vec<String>,
    store: State<'_, TagStore>,
) -> Result<Vec<String>, String> {
    let parsed: Vec<Tag> = tags
        .into_iter()
        .filter_map(|raw| Tag::parse(&raw))
        .collect();
    let mut store = store.0.lock().map_err(|err| err.to_string())?;
    store.set(cidr.clone(), parsed)?;
    Ok(store
        .get(&cidr)
        .into_iter()
        .map(|tag| tag.as_str().to_string())
        .collect())
}

fn append_path(path: &Path, text: &mut String) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|err| err.to_string())?;
    if meta.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path)
            .map_err(|err| err.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|err| err.to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let child = entry.path();
            if child
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
                && child.is_file()
            {
                append_file(&child, text)?;
            }
        }
        return Ok(());
    }
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
    {
        return append_file(path, text);
    }
    Ok(())
}

fn append_file(path: &Path, text: &mut String) -> Result<(), String> {
    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    text.push_str(&contents);
    if !contents.ends_with('\n') {
        text.push('\n');
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_path_reads_txt_files_in_folder() {
        let dir = std::env::temp_dir().join(format!(
            "proxybench-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "192.0.2.10:8080:user:pass\n").unwrap();
        fs::write(dir.join("skip.md"), "nope\n").unwrap();
        let mut text = String::new();
        append_path(&dir, &mut text).unwrap();
        assert!(text.contains("192.0.2.10:8080:user:pass"));
        assert!(!text.contains("nope"));
        let _ = fs::remove_dir_all(&dir);
    }

    fn tagged_store() -> (std::path::PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!(
            "proxybench-rows-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let tags = Store::load(dir.join("tags.json")).unwrap();
        (dir, tags)
    }

    fn sample_bucket(probe: Option<crate::session::StoredProbe>) -> StoredBucket {
        StoredBucket {
            subnet: crate::split::Subnet::from_host("192.0.2.10".parse().unwrap()),
            proxies: vec![crate::parse::ProxyLine {
                host: "192.0.2.10".parse().unwrap(),
                port: 8080,
                username: "user".into(),
                password: "pass".into(),
                source: "192.0.2.10:8080:user:pass".into(),
            }],
            country: Some("FR".into()),
            last_probe: probe,
        }
    }

    #[test]
    fn rows_from_exposes_persisted_metrics() {
        let (dir, tags) = tagged_store();
        let rows = rows_from(
            &[sample_bucket(Some(crate::session::StoredProbe {
                at: 42,
                metrics: Some(crate::session::StoredMetrics {
                    ok: 3,
                    connect_p50: Some(10.0),
                    connect_p95: Some(20.0),
                    ttfb_p50: Some(30.0),
                    ttfb_p95: Some(40.0),
                }),
            }))],
            &tags,
        );
        assert_eq!(rows[0].ok, Some(3));
        assert_eq!(rows[0].connect_p50, Some(10.0));
        assert_eq!(rows[0].connect_p95, Some(20.0));
        assert_eq!(rows[0].ttfb_p50, Some(30.0));
        assert_eq!(rows[0].ttfb_p95, Some(40.0));
        assert_eq!(rows[0].last_run_at, Some(42));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rows_from_keeps_legacy_timestamp_without_metrics() {
        let (dir, tags) = tagged_store();
        let rows = rows_from(
            &[sample_bucket(Some(crate::session::StoredProbe {
                at: 42,
                metrics: None,
            }))],
            &tags,
        );
        assert_eq!(rows[0].ok, None);
        assert_eq!(rows[0].connect_p50, None);
        assert_eq!(rows[0].last_run_at, Some(42));
        let _ = fs::remove_dir_all(&dir);
    }
}

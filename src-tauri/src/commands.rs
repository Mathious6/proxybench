use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::export;
use crate::import::{self, InventoryStore, TagStore};
use crate::last_target;
use crate::run::{self, Metrics, Progress};
use crate::session::{SessionStore, StoredMetrics};
use crate::target::Target;

const LAST_TARGET_FILE: &str = "last-target.txt";
const PROGRESS_EVENT: &str = "run-progress";

pub struct LastTarget(pub last_target::Store);

pub fn open_last_target(app: &AppHandle) -> Result<last_target::Store, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    Ok(last_target::Store::new(dir.join(LAST_TARGET_FILE)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub completed_at: u64,
    pub metrics: Vec<Metrics>,
    pub countries: HashMap<String, String>,
}

#[tauri::command]
pub fn last_target(store: State<'_, LastTarget>) -> Result<Option<String>, String> {
    store.0.load()
}

#[tauri::command]
pub async fn start_run(
    url: String,
    cidrs: Option<Vec<String>>,
    app: AppHandle,
    session: State<'_, SessionStore>,
    last: State<'_, LastTarget>,
    inventory: State<'_, InventoryStore>,
) -> Result<RunResult, String> {
    let target = Target::parse(&url)?;
    last.0.save(&url)?;
    let buckets = {
        let session = session.0.lock().map_err(|err| err.to_string())?;
        if session.is_empty() {
            return Err("Import proxies before probing.".into());
        }
        session.resolve_scope(cidrs)?
    };
    let samples: Vec<_> = buckets
        .iter()
        .filter_map(|bucket| bucket.proxies.first().map(|proxy| proxy.host))
        .collect();
    let window = app.clone();
    let probe = run::probe_session(buckets, target, move |progress: Progress| {
        let _ = window.emit(PROGRESS_EVENT, progress);
    });
    let countries = tauri::async_runtime::spawn_blocking(move || crate::country::lookup(&samples));
    let (finished, countries) = tokio::join!(probe, countries);
    let countries = countries.map_err(|err| err.to_string())?;
    let completed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0);
    {
        let mut session = session.0.lock().map_err(|err| err.to_string())?;
        let mut candidate = session.clone();
        let stored: HashMap<String, StoredMetrics> = finished
            .iter()
            .map(|(cidr, metrics)| (cidr.clone(), stored_metrics(metrics)))
            .collect();
        candidate.record_probes(completed_at, &stored, &countries);
        let snapshot = candidate.snapshot();
        inventory.0.save(&snapshot)?;
        *session = candidate;
    }
    let mut metrics: Vec<Metrics> = finished.into_values().collect();
    metrics.sort_by(|left, right| left.cidr.cmp(&right.cidr));
    Ok(RunResult {
        completed_at,
        metrics,
        countries,
    })
}

fn stored_metrics(metrics: &Metrics) -> StoredMetrics {
    StoredMetrics {
        ok: metrics.ok,
        connect_p50: metrics.connect_p50,
        connect_p95: metrics.connect_p95,
        ttfb_p50: metrics.ttfb_p50,
        ttfb_p95: metrics.ttfb_p95,
    }
}

#[tauri::command]
pub fn export_dir(
    path: String,
    cidrs: Option<Vec<String>>,
    session: State<'_, SessionStore>,
    tags: State<'_, TagStore>,
) -> Result<usize, String> {
    let buckets = {
        let session = session.0.lock().map_err(|err| err.to_string())?;
        if session.is_empty() {
            return Err("Import proxies before exporting.".into());
        }
        session.resolve_scope(cidrs)?
    };
    let store = tags.0.lock().map_err(|err| err.to_string())?;
    export::write_dir(std::path::Path::new(&path), &buckets, &store)
}

#[tauri::command]
pub fn session_rows(
    session: State<'_, SessionStore>,
    tags: State<'_, TagStore>,
) -> Result<Vec<import::SubnetRow>, String> {
    let buckets = session.0.lock().map_err(|err| err.to_string())?.snapshot();
    let tags = tags.0.lock().map_err(|err| err.to_string())?;
    Ok(import::rows_from(&buckets, &tags))
}

#[tauri::command]
pub fn remove_subnet(
    cidr: String,
    session: State<'_, SessionStore>,
    inventory: State<'_, InventoryStore>,
    tags: State<'_, TagStore>,
) -> Result<Vec<import::SubnetRow>, String> {
    let subnet =
        crate::split::Subnet::parse_cidr(&cidr).ok_or_else(|| "Unknown subnet.".to_string())?;
    let snapshot = {
        let mut session = session.0.lock().map_err(|err| err.to_string())?;
        let mut candidate = session.clone();
        if !candidate.remove(subnet) {
            return Err("Unknown subnet.".into());
        }
        let snapshot = candidate.snapshot();
        inventory.0.save(&snapshot)?;
        *session = candidate;
        snapshot
    };
    let tags = tags.0.lock().map_err(|err| err.to_string())?;
    Ok(import::rows_from(&snapshot, &tags))
}

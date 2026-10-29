use std::path::Path;

use tauri::{AppHandle, Emitter, State};

use crate::library;
use crate::settings::{Settings, SettingsSheet};
use crate::state::AppState;
use crate::wire::{ImportReport, RunProgress, RunSummary, SubnetRow};
use crate::workflow;

const PROGRESS_EVENT: &str = "run-progress";

#[tauri::command]
pub fn rows(state: State<'_, AppState>) -> Result<Vec<SubnetRow>, String> {
    library::rows(&state)
}

#[tauri::command]
pub async fn import_paths(
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<ImportReport, String> {
    library::import_paths(&state, paths).await
}

#[tauri::command]
pub fn set_tags(
    cidr: String,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    library::set_tags(&state, cidr, tags)
}

#[tauri::command]
pub fn remove(cidrs: Vec<String>, state: State<'_, AppState>) -> Result<Vec<SubnetRow>, String> {
    library::remove(&state, cidrs)
}

#[tauri::command]
pub fn settings(state: State<'_, AppState>) -> Result<SettingsSheet, String> {
    state.settings().map(SettingsSheet::new)
}

#[tauri::command]
pub fn save_settings(
    settings: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    let settings = serde_json::from_value(settings).map_err(|err| err.to_string())?;
    state.replace_settings(settings)
}

#[tauri::command]
pub async fn check(
    cidrs: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RunSummary, String> {
    workflow::check(&state, cidrs, |progress| emit_progress(&app, progress)).await
}

#[tauri::command]
pub async fn probe(
    cidrs: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RunSummary, String> {
    workflow::probe(&state, cidrs, |progress| emit_progress(&app, progress)).await
}

#[tauri::command]
pub fn stop(state: State<'_, AppState>) {
    state.slot.stop();
}

#[tauri::command]
pub fn export_txt(
    path: String,
    cidrs: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    library::export_txt(&state, Path::new(&path), cidrs)
}

#[tauri::command]
pub fn export_aycd(
    path: String,
    cidrs: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    library::export_aycd(&state, Path::new(&path), cidrs)
}

fn emit_progress(app: &AppHandle, progress: RunProgress) {
    let _ = app.emit(PROGRESS_EVENT, progress);
}

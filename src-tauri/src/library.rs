use std::net::Ipv4Addr;
use std::path::Path;

use crate::aycd;
use crate::country;
use crate::export;
use crate::import;
use crate::inventory::{self, Bucket};
use crate::state::AppState;
use crate::subnet::{self, Subnet};
use crate::tag::Tag;
use crate::wire::{ImportReport, SubnetRow};

pub fn rows(state: &AppState) -> Result<Vec<SubnetRow>, String> {
    let inventory = state.inventory()?;
    let tags = state.tags()?;
    Ok(SubnetRow::list(&inventory.buckets(), &tags))
}

pub async fn import_paths(state: &AppState, paths: Vec<String>) -> Result<ImportReport, String> {
    let (proxies, skipped) = import::read_proxies(&paths)?;
    let groups = subnet::by_slash24(proxies);
    let unlocated: Vec<Ipv4Addr> = {
        let inventory = state.inventory()?;
        groups
            .iter()
            .filter(|group| {
                inventory
                    .get(group.subnet)
                    .is_none_or(|bucket| bucket.country.is_none())
            })
            .filter_map(|group| group.proxies.first().map(|proxy| proxy.host))
            .collect()
    };
    let countries = tauri::async_runtime::spawn_blocking(move || country::lookup(&unlocated))
        .await
        .map_err(|err| err.to_string())?;
    let merge = state.update_inventory(|inventory| inventory.merge(groups, &countries))?;
    Ok(ImportReport {
        rows: rows(state)?,
        added: merge.added,
        grown: merge.grown,
        skipped,
    })
}

pub fn set_tags(state: &AppState, cidr: String, tags: Vec<String>) -> Result<Vec<String>, String> {
    let parsed: Vec<Tag> = tags.iter().filter_map(|raw| Tag::parse(raw)).collect();
    let mut book = state.tags()?;
    book.set(cidr.clone(), parsed)?;
    Ok(book
        .get(&cidr)
        .iter()
        .map(|tag| tag.as_str().to_string())
        .collect())
}

pub fn remove(state: &AppState, cidrs: Vec<String>) -> Result<Vec<SubnetRow>, String> {
    let subnets: Vec<Subnet> = inventory::parse_subnets(&cidrs)?.into_iter().collect();
    state.update_inventory(|inventory| inventory.remove(&subnets))?;
    rows(state)
}

pub fn export_txt(
    state: &AppState,
    path: &Path,
    cidrs: Option<Vec<String>>,
) -> Result<usize, String> {
    let buckets = export_scope(state, cidrs)?;
    export::write_dir(path, &buckets, &*state.tags()?)
}

pub fn export_aycd(
    state: &AppState,
    path: &Path,
    cidrs: Option<Vec<String>>,
) -> Result<usize, String> {
    let buckets = export_scope(state, cidrs)?;
    aycd::write(path, &buckets, &*state.tags()?)
}

fn export_scope(state: &AppState, cidrs: Option<Vec<String>>) -> Result<Vec<Bucket>, String> {
    state.inventory()?.scope(cidrs)
}

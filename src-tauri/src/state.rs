use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::inventory::Inventory;
use crate::inventory_file::InventoryFile;
use crate::settings::{Settings, SettingsFile};
use crate::slot::RunSlot;
use crate::tag::TagBook;

const INVENTORY_FILE: &str = "proxies.json";
const TAGS_FILE: &str = "tags.json";
const SETTINGS_FILE: &str = "settings.json";
const UNAVAILABLE: &str = "App state is unavailable.";

pub struct AppState {
    inventory: Mutex<Inventory>,
    tags: Mutex<TagBook>,
    settings: Mutex<Settings>,
    inventory_file: InventoryFile,
    settings_file: SettingsFile,
    pub slot: RunSlot,
}

impl AppState {
    pub fn open(dir: &Path) -> Result<Self, String> {
        let inventory_file = InventoryFile::new(dir.join(INVENTORY_FILE));
        let settings_file = SettingsFile::new(dir.join(SETTINGS_FILE));
        Ok(Self {
            inventory: Mutex::new(Inventory::restore(inventory_file.load()?)),
            tags: Mutex::new(TagBook::load(dir.join(TAGS_FILE))?),
            settings: Mutex::new(settings_file.load()?),
            inventory_file,
            settings_file,
            slot: RunSlot::new(),
        })
    }

    pub fn inventory(&self) -> Result<MutexGuard<'_, Inventory>, String> {
        self.inventory.lock().map_err(|_| UNAVAILABLE.to_string())
    }

    pub fn tags(&self) -> Result<MutexGuard<'_, TagBook>, String> {
        self.tags.lock().map_err(|_| UNAVAILABLE.to_string())
    }

    pub fn settings(&self) -> Result<Settings, String> {
        self.settings
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| UNAVAILABLE.to_string())
    }

    pub fn replace_settings(&self, next: Settings) -> Result<Settings, String> {
        let mut settings = self.settings.lock().map_err(|_| UNAVAILABLE.to_string())?;
        self.settings_file.save(&next)?;
        *settings = next.clone();
        Ok(next)
    }

    pub fn update_inventory<R>(
        &self,
        change: impl FnOnce(&mut Inventory) -> R,
    ) -> Result<R, String> {
        let mut inventory = self.inventory()?;
        let mut candidate = inventory.clone();
        let result = change(&mut candidate);
        self.inventory_file.save(&candidate.buckets())?;
        *inventory = candidate;
        Ok(result)
    }

    pub fn persist_inventory(&self) -> Result<(), String> {
        let buckets = self.inventory()?.buckets();
        self.inventory_file.save(&buckets)
    }
}

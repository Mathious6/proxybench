use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::target::Target;

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Option<String>, String> {
        match fs::read_to_string(&self.path) {
            Ok(raw) => {
                let value = raw.trim();
                if value.is_empty() {
                    return Ok(None);
                }
                match Target::parse(value) {
                    Ok(_) => Ok(Some(value.to_string())),
                    Err(_) => Ok(None),
                }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn save(&self, url: &str) -> Result<(), String> {
        let _ = Target::parse(url)?;
        write(&self.path, url.trim())
    }
}

fn write(path: &Path, url: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, url).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!(
            "proxybench-last-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("last-target.txt");
        (dir, Store::new(path))
    }

    #[test]
    fn save_then_load_trims_and_keeps_https() {
        let (dir, store) = temp_store();
        store.save("  https://example.com/status  ").unwrap();
        assert_eq!(
            store.load().unwrap().as_deref(),
            Some("https://example.com/status")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_rejects_non_https() {
        let (dir, store) = temp_store();
        assert!(store.save("http://example.com").is_err());
        assert!(store.load().unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_is_none() {
        let (dir, store) = temp_store();
        assert!(store.load().unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_ignores_corrupt_saved_url() {
        let (dir, store) = temp_store();
        fs::create_dir_all(&dir).unwrap();
        fs::write(&store.path, "not-https").unwrap();
        assert!(store.load().unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}

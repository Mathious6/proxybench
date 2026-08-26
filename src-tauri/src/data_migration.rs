use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const FILES: [&str; 3] = ["proxies.json", "tags.json", "last-target.txt"];
const OLD_IDENTIFIER: &str = "com.proxybench.app";
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

pub fn migrate(data_dir: &Path) -> Result<(), String> {
    let Some(parent) = data_dir.parent() else {
        return Ok(());
    };
    let old_dir = parent.join(OLD_IDENTIFIER);
    for name in FILES {
        copy_missing(&old_dir.join(name), &data_dir.join(name))?;
    }
    Ok(())
}

fn copy_missing(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() || !source.is_file() {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let temporary = destination.with_extension(format!(
        "migrating-{}-{}",
        std::process::id(),
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    fs::copy(source, &temporary).map_err(|err| err.to_string())?;
    let result = match fs::hard_link(&temporary, destination) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err.to_string()),
    };
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "proxybench-data-migration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn copies_existing_data_to_new_identifier_directory() {
        let root = temp_dir();
        let old = root.join(OLD_IDENTIFIER);
        let new = root.join("com.mathious.proxybench");
        fs::create_dir_all(&old).unwrap();
        for name in FILES {
            fs::write(old.join(name), name).unwrap();
        }

        migrate(&new).unwrap();

        for name in FILES {
            assert_eq!(fs::read_to_string(new.join(name)).unwrap(), name);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_data_already_present_in_new_directory() {
        let root = temp_dir();
        let old = root.join(OLD_IDENTIFIER);
        let new = root.join("com.mathious.proxybench");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("proxies.json"), "old").unwrap();
        fs::write(new.join("proxies.json"), "new").unwrap();

        migrate(&new).unwrap();

        assert_eq!(fs::read_to_string(new.join("proxies.json")).unwrap(), "new");
        fs::remove_dir_all(root).unwrap();
    }
}

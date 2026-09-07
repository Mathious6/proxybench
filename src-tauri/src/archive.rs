use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

pub(crate) fn write_zip(path: &Path, entries: &[(String, String)]) -> Result<(), String> {
    let file = crate::secure_file::open_truncating(path).map_err(|err| io_error(path, err))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    for (name, text) in entries {
        writer
            .start_file(name.as_str(), options)
            .map_err(|err| zip_error(path, err))?;
        writer
            .write_all(text.as_bytes())
            .map_err(|err| io_error(path, err))?;
    }
    writer.finish().map_err(|err| zip_error(path, err))?;
    Ok(())
}

fn io_error(path: &Path, err: std::io::Error) -> String {
    format!("Could not write {} ({})", path.display(), err.kind())
}

fn zip_error(path: &Path, err: zip::result::ZipError) -> String {
    format!("Could not write {} ({})", path.display(), err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;
    use std::path::PathBuf;
    use zip::ZipArchive;

    fn entries() -> Vec<(String, String)> {
        vec![
            (
                "FR_192.0.2.0_24_2.txt".into(),
                "192.0.2.10:8080:a:b\n192.0.2.11:8080:a:c\n".into(),
            ),
            (
                "XX_198.51.100.0_24_1.txt".into(),
                "198.51.100.7:8080:d:e\n".into(),
            ),
            (
                "ALL_3.txt".into(),
                "192.0.2.10:8080:a:b\n192.0.2.11:8080:a:c\n198.51.100.7:8080:d:e\n".into(),
            ),
        ]
    }

    fn temp_path(extension: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "proxybench-archive-{}-{}-{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            extension
        ))
    }

    #[test]
    fn write_zip_round_trips_every_named_entry() {
        let path = temp_path("zip");
        write_zip(&path, &entries()).unwrap();
        let mut archive = ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(archive.len(), 3);
        for (name, text) in entries() {
            let mut stored = archive.by_name(&name).unwrap();
            let mut body = String::new();
            stored.read_to_string(&mut body).unwrap();
            assert_eq!(body, text);
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn write_zip_overwrites_an_existing_archive() {
        let path = temp_path("zip");
        let stale = vec![("old.txt".into(), "gone\n".into())];
        write_zip(&path, &stale).unwrap();
        write_zip(&path, &entries()).unwrap();
        let archive = ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(archive.len(), 3);
        assert!(archive.file_names().all(|name| !name.eq("old.txt")));
        let _ = fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn write_zip_keeps_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path("zip");
        write_zip(&path, &entries()).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_file(path);
    }
}

use std::fs;
use std::path::Path;

use crate::proxy::{self, ProxyLine};

pub fn read_proxies(paths: &[String]) -> Result<(Vec<ProxyLine>, usize), String> {
    let mut text = String::new();
    for path in paths {
        append_path(Path::new(path), &mut text)?;
    }
    Ok(proxy::parse_text(&text))
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
    fn read_proxies_counts_skipped_lines_across_paths() {
        let dir = std::env::temp_dir().join(format!(
            "proxybench-import-skip-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let first = dir.join("a.txt");
        let second = dir.join("b.txt");
        fs::write(&first, "192.0.2.10:8080:user:pass\nnope").unwrap();
        fs::write(&second, "198.51.100.2:8080:user:pass\n").unwrap();
        let (proxies, skipped) = read_proxies(&[
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert_eq!(proxies.len(), 2);
        assert_eq!(skipped, 1);
        let _ = fs::remove_dir_all(&dir);
    }

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
}

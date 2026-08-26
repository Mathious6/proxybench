use std::fs;
use std::io::Write;
use std::path::Path;

use crate::session::StoredBucket;
use crate::tags::{Store, Tag};

pub fn filename(tags: &[Tag], country: Option<&str>, bucket: &StoredBucket) -> String {
    let country = country
        .filter(|code| code.len() == 2 && code.bytes().all(|b| b.is_ascii_alphabetic()))
        .map(|code| code.to_ascii_uppercase())
        .unwrap_or_else(|| "XX".into());
    let ip = bucket.subnet.network();
    let qty = bucket.proxies.len();
    match tag_stem(tags) {
        Some(tags) => format!("{tags}_{country}_{ip}_24_{qty}.txt"),
        None => format!("{country}_{ip}_24_{qty}.txt"),
    }
}

fn tag_stem(tags: &[Tag]) -> Option<String> {
    let parts: Vec<String> = tags
        .iter()
        .map(|tag| sanitize(tag.as_str()))
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("-"))
    }
}

fn sanitize(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_ascii_whitespace()) && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn write_dir(dir: &Path, buckets: &[StoredBucket], tags: &Store) -> Result<usize, String> {
    if buckets.is_empty() {
        return Err("Import proxies before exporting.".into());
    }
    fs::create_dir_all(dir).map_err(|err| io_error(dir, err))?;
    let mut written = 0;
    for bucket in buckets {
        let cidr = bucket.subnet.cidr();
        let name = filename(&tags.get(&cidr), bucket.country.as_deref(), bucket);
        let path = dir.join(&name);
        write_file(&path, bucket)?;
        written += 1;
    }
    Ok(written)
}

fn write_file(path: &Path, bucket: &StoredBucket) -> Result<(), String> {
    let mut body = String::new();
    for proxy in &bucket.proxies {
        body.push_str(&proxy.source);
        body.push('\n');
    }
    write_mode(path, body.as_bytes()).map_err(|err| io_error(path, err))
}

fn io_error(path: &Path, err: std::io::Error) -> String {
    format!("Could not write {} ({})", path.display(), err.kind())
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
    use std::path::PathBuf;

    use crate::parse::ProxyLine;
    use crate::split::Subnet;

    fn bucket(host: &str, n: usize) -> StoredBucket {
        StoredBucket {
            subnet: Subnet::from_host(host.parse().unwrap()),
            proxies: (0..n)
                .map(|i| ProxyLine {
                    host: host.parse().unwrap(),
                    port: 8080,
                    username: "user".into(),
                    password: "pass".into(),
                    source: format!("{host}:8080:user:{i}"),
                })
                .collect(),
            country: Some("FR".into()),
            last_probe: None,
        }
    }

    fn tags(values: &[&str]) -> Vec<Tag> {
        values
            .iter()
            .filter_map(|value| Tag::parse(value))
            .collect()
    }

    #[test]
    fn filename_joins_sanitized_tags_country_ip_and_qty() {
        let name = filename(
            &tags(&["isp", "mobile"]),
            Some("fr"),
            &bucket("51.194.38.2", 42),
        );
        assert_eq!(name, "isp-mobile_FR_51.194.38.0_24_42.txt");
    }

    #[test]
    fn filename_starts_with_country_when_tags_are_missing() {
        let name = filename(&[], None, &bucket("192.0.2.10", 1));
        assert_eq!(name, "XX_192.0.2.0_24_1.txt");
        let named = filename(&[], Some("FR"), &bucket("192.0.2.10", 1));
        assert_eq!(named, "FR_192.0.2.0_24_1.txt");
    }

    #[test]
    fn filename_strips_unsafe_tag_characters() {
        let name = filename(
            &tags(&["ISP Mobile!", "../x"]),
            Some("US"),
            &bucket("198.51.100.2", 3),
        );
        assert_eq!(name, "isp-mobile-x_US_198.51.100.0_24_3.txt");
    }

    fn temp_dir() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "proxybench-export-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_dir_writes_verbatim_sources() {
        let dir = temp_dir();
        let mut bucket = bucket("192.0.2.10", 0);
        bucket.proxies.push(ProxyLine {
            host: "192.0.2.10".parse().unwrap(),
            port: 8080,
            username: "user".into(),
            password: "pass".into(),
            source: "  192.0.2.10:8080:user:p:ss  ".into(),
        });
        bucket.country = Some("FR".into());
        let tags_path = dir.join("tags.json");
        let mut store = Store::load(tags_path).unwrap();
        store.set("192.0.2.0/24".into(), tags(&["isp"])).unwrap();
        let written = write_dir(&dir.join("out"), &[bucket], &store).unwrap();
        assert_eq!(written, 1);
        let body = fs::read_to_string(dir.join("out").join("isp_FR_192.0.2.0_24_1.txt")).unwrap();
        assert_eq!(body, "  192.0.2.10:8080:user:p:ss  \n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_dir_rejects_empty_session() {
        let dir = temp_dir();
        let store = Store::load(dir.join("tags.json")).unwrap();
        assert!(write_dir(&dir.join("out"), &[], &store).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_dir_writes_one_bucket_when_given_one() {
        let dir = temp_dir();
        let store = Store::load(dir.join("tags.json")).unwrap();
        let written = write_dir(
            &dir.join("out"),
            &[bucket("192.0.2.10", 1), bucket("198.51.100.2", 2)][0..1],
            &store,
        )
        .unwrap();
        assert_eq!(written, 1);
        assert!(dir.join("out").join("FR_192.0.2.0_24_1.txt").exists());
        assert!(!dir.join("out").join("FR_198.51.100.0_24_2.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_dir_sets_owner_only_permissions() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let dir = temp_dir();
        let out = dir.join("out");
        fs::create_dir_all(&out).unwrap();
        let path = out.join("FR_192.0.2.0_24_1.txt");
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&path)
            .unwrap();
        let store = Store::load(dir.join("tags.json")).unwrap();
        write_dir(&out, &[bucket("192.0.2.10", 1)], &store).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }
}

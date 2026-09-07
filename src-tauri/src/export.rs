use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use crate::session::StoredBucket;
use crate::tags::{Store, Tag};

pub fn filename(tags: &[Tag], country: Option<&str>, bucket: &StoredBucket) -> String {
    format!("{}.txt", category(tags, country, bucket))
}

pub fn category(tags: &[Tag], country: Option<&str>, bucket: &StoredBucket) -> String {
    let country = country
        .and_then(valid_country)
        .unwrap_or_else(|| "XX".into());
    let ip = bucket.subnet.network();
    let qty = bucket.proxies.len();
    match tag_stem(tags) {
        Some(tags) => format!("{tags}_{country}_{ip}_24_{qty}"),
        None => format!("{country}_{ip}_24_{qty}"),
    }
}

fn valid_country(code: &str) -> Option<String> {
    (code.len() == 2 && code.bytes().all(|b| b.is_ascii_alphabetic()))
        .then(|| code.to_ascii_uppercase())
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

struct Bundle {
    filename: String,
    entries: Vec<(String, String)>,
}

fn bundle(buckets: &[StoredBucket], tags: &Store) -> Result<Bundle, String> {
    if buckets.is_empty() {
        return Err("Import proxies before exporting.".into());
    }
    let total: usize = buckets.iter().map(|bucket| bucket.proxies.len()).sum();
    let mut entries: Vec<(String, String)> = buckets
        .iter()
        .map(|bucket| {
            (
                filename(
                    &tags.get(&bucket.subnet.cidr()),
                    bucket.country.as_deref(),
                    bucket,
                ),
                body(bucket),
            )
        })
        .collect();
    entries.push((
        format!("ALL_{total}.txt"),
        entries.iter().map(|(_, text)| text.as_str()).collect(),
    ));
    Ok(Bundle {
        filename: bundle_filename(buckets, tags, total),
        entries,
    })
}

fn bundle_filename(buckets: &[StoredBucket], tags: &Store, total: usize) -> String {
    format!(
        "{}_{}subnets_{}proxies_{}.zip",
        bundle_prefix(buckets, tags),
        buckets.len(),
        total,
        fingerprint(buckets)
    )
}

fn fingerprint(buckets: &[StoredBucket]) -> String {
    let mut cidrs: Vec<String> = buckets.iter().map(|b| b.subnet.cidr()).collect();
    cidrs.sort();
    let mut hasher = DefaultHasher::new();
    cidrs.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn bundle_prefix(buckets: &[StoredBucket], tags: &Store) -> String {
    if let Some(stem) = uniform_tag_stem(buckets, tags) {
        return stem;
    }
    if let Some(code) = uniform_country(buckets) {
        return code;
    }
    "MIXED".into()
}

fn uniform_tag_stem(buckets: &[StoredBucket], tags: &Store) -> Option<String> {
    let mut stems = buckets
        .iter()
        .map(|bucket| tag_stem(&tags.get(&bucket.subnet.cidr())));
    let stem = stems.next()?.as_ref()?.clone();
    stems
        .all(|other| other.as_deref() == Some(stem.as_str()))
        .then_some(stem)
}

fn uniform_country(buckets: &[StoredBucket]) -> Option<String> {
    let mut codes = buckets
        .iter()
        .map(|bucket| bucket.country.as_deref().and_then(valid_country));
    let first = codes.next()?;
    if !codes.all(|code| code == first) {
        return None;
    }
    Some(first.unwrap_or_else(|| "XX".into()))
}

pub fn write_dir(dir: &Path, buckets: &[StoredBucket], tags: &Store) -> Result<usize, String> {
    if buckets.is_empty() {
        return Err("Import proxies before exporting.".into());
    }
    fs::create_dir_all(dir).map_err(|err| io_error(dir, err))?;
    match buckets {
        [single] => {
            let path = dir.join(filename(
                &tags.get(&single.subnet.cidr()),
                single.country.as_deref(),
                single,
            ));
            write_text(&path, &body(single))?;
            Ok(1)
        }
        many => {
            let bundle = bundle(many, tags)?;
            let path = dir.join(&bundle.filename);
            crate::archive::write_zip(&path, &bundle.entries)?;
            Ok(1)
        }
    }
}

fn body(bucket: &StoredBucket) -> String {
    let mut text = String::new();
    for proxy in &bucket.proxies {
        text.push_str(&proxy.source);
        text.push('\n');
    }
    text
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    crate::secure_file::write(path, text.as_bytes()).map_err(|err| io_error(path, err))
}

fn io_error(path: &Path, err: std::io::Error) -> String {
    format!("Could not write {} ({})", path.display(), err.kind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};
    use std::path::PathBuf;
    use zip::ZipArchive;

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
    fn category_is_the_filename_without_the_extension() {
        let bucket = bucket("51.194.38.2", 42);
        let category = category(&tags(&["isp", "mobile"]), Some("fr"), &bucket);
        assert_eq!(
            filename(&tags(&["isp", "mobile"]), Some("fr"), &bucket),
            format!("{category}.txt")
        );
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

    #[test]
    fn bundle_names_uniform_tagged_scopes_after_the_shared_tag_stem() {
        let dir = temp_dir();
        let mut store = Store::load(dir.join("tags.json")).unwrap();
        store.set("192.0.2.0/24".into(), tags(&["isp"])).unwrap();
        store.set("198.51.100.0/24".into(), tags(&["isp"])).unwrap();
        let bundle = bundle(
            &[bucket("192.0.2.10", 2), bucket("198.51.100.2", 3)],
            &store,
        )
        .unwrap();
        assert!(bundle.filename.starts_with("isp_2subnets_5proxies_"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_names_partially_known_countries_as_mixed() {
        let dir = temp_dir();
        let store = Store::load(dir.join("tags.json")).unwrap();
        let mut first = bucket("192.0.2.10", 1);
        first.country = Some("FR".into());
        let mut second = bucket("198.51.100.2", 1);
        second.country = None;
        let bundle = bundle(&[first, second], &store).unwrap();
        assert!(bundle.filename.starts_with("MIXED_2subnets_2proxies_"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_names_country_less_scopes_xx() {
        let dir = temp_dir();
        let store = Store::load(dir.join("tags.json")).unwrap();
        let mut first = bucket("192.0.2.10", 1);
        first.country = None;
        let mut second = bucket("198.51.100.2", 2);
        second.country = None;
        let bundle = bundle(&[first, second], &store).unwrap();
        assert!(bundle.filename.starts_with("XX_2subnets_3proxies_"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_collects_per_subnet_entries_plus_the_combined_import_list() {
        let dir = temp_dir();
        let store = Store::load(dir.join("tags.json")).unwrap();
        let bundle = bundle(
            &[bucket("192.0.2.10", 2), bucket("198.51.100.2", 1)],
            &store,
        )
        .unwrap();
        assert_eq!(bundle.entries.len(), 3);
        assert_eq!(
            bundle.entries[0],
            (
                "FR_192.0.2.0_24_2.txt".into(),
                "192.0.2.10:8080:user:0\n192.0.2.10:8080:user:1\n".into()
            )
        );
        assert_eq!(
            bundle.entries[1],
            (
                "FR_198.51.100.0_24_1.txt".into(),
                "198.51.100.2:8080:user:0\n".into()
            )
        );
        assert_eq!(bundle.entries[2].0, "ALL_3.txt");
        assert_eq!(
            bundle.entries[2].1,
            format!("{}{}", bundle.entries[0].1, bundle.entries[1].1)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_filenames_distinguish_distinct_scopes_sharing_the_same_aggregates() {
        let dir = temp_dir();
        let store = Store::load(dir.join("tags.json")).unwrap();
        let first = bundle(
            &[bucket("192.0.2.10", 3), bucket("198.51.100.2", 3)],
            &store,
        )
        .unwrap();
        let second = bundle(
            &[bucket("192.0.2.10", 1), bucket("203.0.113.10", 5)],
            &store,
        )
        .unwrap();
        assert!(first.filename.starts_with("FR_2subnets_6proxies_"));
        assert!(second.filename.starts_with("FR_2subnets_6proxies_"));
        assert_ne!(first.filename, second.filename);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_filenames_follow_the_scope_not_the_member_content() {
        let dir = temp_dir();
        let store = Store::load(dir.join("tags.json")).unwrap();
        let first = bundle(
            &[bucket("192.0.2.10", 2), bucket("198.51.100.2", 1)],
            &store,
        )
        .unwrap();
        let second = bundle(
            &[bucket("192.0.2.10", 3), bucket("198.51.100.2", 0)],
            &store,
        )
        .unwrap();
        assert_eq!(first.filename, second.filename);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_dir_writes_a_single_bundle_zip_when_several_buckets_are_selected() {
        let dir = temp_dir();
        let out = dir.join("out");
        let store = Store::load(dir.join("tags.json")).unwrap();
        let written = write_dir(
            &out,
            &[bucket("192.0.2.10", 1), bucket("198.51.100.2", 2)],
            &store,
        )
        .unwrap();
        assert_eq!(written, 1);
        let produced: Vec<String> = fs::read_dir(&out)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(produced.len(), 1);
        let zip_name = produced.first().unwrap();
        assert!(zip_name.starts_with("FR_2subnets_3proxies_"));
        assert!(zip_name.ends_with(".zip"));
        let bytes = fs::read(out.join(zip_name)).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 3);
        let mut combined = archive.by_name("ALL_3.txt").unwrap();
        let mut body = String::new();
        combined.read_to_string(&mut body).unwrap();
        assert_eq!(
            body,
            "192.0.2.10:8080:user:0\n198.51.100.2:8080:user:0\n198.51.100.2:8080:user:1\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

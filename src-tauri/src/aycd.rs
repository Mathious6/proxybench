use std::path::Path;

use serde::Serialize;
use uuid::Uuid;

use crate::export;
use crate::session::StoredBucket;
use crate::tags::Store;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Proxy {
    id: String,
    all_index: i8,
    category_index: i8,
    category: String,
    url: String,
    port: String,
    user: String,
    pass: String,
    #[serde(rename = "type")]
    kind: &'static str,
}

pub fn write(path: &Path, buckets: &[StoredBucket], tags: &Store) -> Result<usize, String> {
    if buckets.is_empty() {
        return Err("Import proxies before exporting.".into());
    }
    let proxies: Vec<Proxy> = buckets
        .iter()
        .flat_map(|bucket| {
            let category = export::category(
                &tags.get(&bucket.subnet.cidr()),
                bucket.country.as_deref(),
                bucket,
            );
            bucket.proxies.iter().map(move |proxy| Proxy {
                id: Uuid::new_v4().to_string(),
                all_index: -1,
                category_index: -1,
                category: category.clone(),
                url: proxy.host.to_string(),
                port: proxy.port.to_string(),
                user: proxy.username.clone(),
                pass: proxy.password.clone(),
                kind: "HTTP",
            })
        })
        .collect();
    let bytes = serde_json::to_vec_pretty(&proxies).map_err(|err| err.to_string())?;
    let path = json_path(path);
    crate::secure_file::write(&path, &bytes).map_err(|err| io_error(&path, err))?;
    Ok(proxies.len())
}

fn json_path(path: &Path) -> std::path::PathBuf {
    if path.extension().and_then(|value| value.to_str()) == Some("json") {
        path.to_owned()
    } else {
        let mut value = path.as_os_str().to_owned();
        value.push(".json");
        value.into()
    }
}

fn io_error(path: &Path, err: std::io::Error) -> String {
    format!("Could not write {} ({})", path.display(), err.kind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ProxyLine;
    use crate::split::Subnet;
    use crate::tags::Tag;
    use std::fs;

    fn bucket(host: &str, country: Option<&str>, password: &str) -> StoredBucket {
        StoredBucket {
            subnet: Subnet::from_host(host.parse().unwrap()),
            proxies: vec![ProxyLine {
                host: host.parse().unwrap(),
                port: 8080,
                username: "user".into(),
                password: password.into(),
                source: String::new(),
            }],
            country: country.map(str::to_owned),
            last_probe: None,
        }
    }

    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "proxybench-aycd-{}-{}.json",
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    #[test]
    fn write_serializes_aycd_schema_with_categories_and_unique_v4_ids() {
        let path = temp_path();
        let tags_path = temp_path();
        let mut tags = Store::load(tags_path.clone()).unwrap();
        tags.set(
            "192.0.2.0/24".into(),
            vec![Tag::parse("Residential").unwrap()],
        )
        .unwrap();
        let buckets = vec![
            bucket("192.0.2.10", Some("fr"), "p:ss"),
            bucket("198.51.100.10", None, "pass"),
        ];
        assert_eq!(write(&path, &buckets, &tags).unwrap(), 2);
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.windows(4).any(|window| window == b"\n  {"));
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["allIndex"], -1);
        assert_eq!(rows[0]["categoryIndex"], -1);
        assert_eq!(rows[0]["category"], "residential_FR_192.0.2.0_24_1");
        assert_eq!(rows[0]["url"], "192.0.2.10");
        assert_eq!(rows[0]["port"], "8080");
        assert_eq!(rows[0]["user"], "user");
        assert_eq!(rows[0]["pass"], "p:ss");
        assert_eq!(rows[0]["type"], "HTTP");
        assert_eq!(rows[1]["category"], "XX_198.51.100.0_24_1");
        let first = rows[0]["id"].as_str().unwrap();
        let second = rows[1]["id"].as_str().unwrap();
        assert_ne!(first, second);
        assert_eq!(Uuid::parse_str(first).unwrap().get_version_num(), 4);
        assert_eq!(Uuid::parse_str(second).unwrap().get_version_num(), 4);
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(tags_path);
    }

    #[test]
    fn write_appends_json_extension() {
        let path = temp_path().with_extension("txt");
        let expected = json_path(&path);
        assert!(expected.to_string_lossy().ends_with(".txt.json"));
        let tags_path = temp_path();
        let tags = Store::load(tags_path.clone()).unwrap();
        write(&path, &[bucket("192.0.2.10", Some("FR"), "pass")], &tags).unwrap();
        assert!(expected.exists());
        let _ = fs::remove_file(expected);
        let _ = fs::remove_file(tags_path);
    }

    #[cfg(unix)]
    #[test]
    fn write_sets_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path();
        let tags_path = temp_path();
        let tags = Store::load(tags_path.clone()).unwrap();
        write(&path, &[bucket("192.0.2.10", Some("FR"), "pass")], &tags).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(tags_path);
    }
}

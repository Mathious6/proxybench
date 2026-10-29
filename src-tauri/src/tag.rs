use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Clone, PartialEq, Eq)]
pub struct Tag(String);

impl Tag {
    pub fn parse(raw: &str) -> Option<Self> {
        let value = raw.trim();
        if value.is_empty() {
            return None;
        }
        Some(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Disk {
    tags: HashMap<String, Vec<String>>,
}

pub struct Store {
    path: PathBuf,
    tags: HashMap<String, Vec<Tag>>,
}

impl Store {
    pub fn load(path: PathBuf) -> Result<Self, String> {
        match read(&path) {
            Ok(tags) => Ok(Self { path, tags }),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Self {
                path,
                tags: HashMap::new(),
            }),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn get(&self, cidr: &str) -> Vec<Tag> {
        self.tags.get(cidr).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, cidr: String, tags: Vec<Tag>) -> Result<(), String> {
        let mut next = self.tags.clone();
        if tags.is_empty() {
            next.remove(&cidr);
        } else {
            next.insert(cidr, unique(tags));
        }
        write(&self.path, &next)?;
        self.tags = next;
        Ok(())
    }
}

fn unique(tags: Vec<Tag>) -> Vec<Tag> {
    let mut seen = Vec::new();
    for tag in tags {
        if !seen
            .iter()
            .any(|existing: &Tag| existing.0.eq_ignore_ascii_case(&tag.0))
        {
            seen.push(tag);
        }
    }
    seen
}

fn read(path: &Path) -> Result<HashMap<String, Vec<Tag>>, std::io::Error> {
    let bytes = fs::read(path)?;
    let disk: Disk = serde_json::from_slice(&bytes)
        .map_err(|err| std::io::Error::new(ErrorKind::InvalidData, err))?;
    Ok(disk
        .tags
        .into_iter()
        .map(|(cidr, values)| {
            (
                cidr,
                values
                    .into_iter()
                    .filter_map(|value| Tag::parse(&value))
                    .collect(),
            )
        })
        .collect())
}

fn write(path: &Path, tags: &HashMap<String, Vec<Tag>>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let disk = Disk {
        tags: tags
            .iter()
            .map(|(cidr, values)| {
                (
                    cidr.clone(),
                    values.iter().map(|tag| tag.0.clone()).collect(),
                )
            })
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&disk).map_err(|err| err.to_string())?;
    fs::write(path, bytes).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "proxybench-tags-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("tags.json");
        (dir, path)
    }

    #[test]
    fn parse_trims_and_rejects_empty() {
        assert_eq!(Tag::parse(" isp ").unwrap().as_str(), "isp");
        assert!(Tag::parse("  ").is_none());
    }

    #[test]
    fn unique_drops_case_insensitive_duplicates() {
        let tags = unique(vec![
            Tag::parse("isp").unwrap(),
            Tag::parse("ISP").unwrap(),
            Tag::parse("mobile").unwrap(),
        ]);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].as_str(), "isp");
        assert_eq!(tags[1].as_str(), "mobile");
    }

    #[test]
    fn store_roundtrips_and_clears_empty() {
        let (dir, path) = temp_store();
        let mut store = Store::load(path.clone()).unwrap();
        store
            .set("192.0.2.0/24".into(), vec![Tag::parse("isp").unwrap()])
            .unwrap();
        let reloaded = Store::load(path.clone()).unwrap();
        assert_eq!(reloaded.get("192.0.2.0/24")[0].as_str(), "isp");
        assert!(reloaded.get("198.51.100.0/24").is_empty());
        let mut store = Store::load(path.clone()).unwrap();
        store.set("192.0.2.0/24".into(), vec![]).unwrap();
        let cleared = Store::load(path).unwrap();
        assert!(cleared.get("192.0.2.0/24").is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_corrupt_json() {
        let (dir, path) = temp_store();
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "{not json").unwrap();
        assert!(Store::load(path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}

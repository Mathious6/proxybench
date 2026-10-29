use serde::Serialize;

use crate::health::Health;
use crate::inventory::Bucket;
use crate::measurement::Probe;
use crate::tag::TagBook;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubnetRow {
    pub cidr: String,
    pub country: Option<String>,
    pub quantity: usize,
    pub tags: Vec<String>,
    pub health: Option<Health>,
    pub probe: Option<Probe>,
}

impl SubnetRow {
    pub fn new(bucket: &Bucket, tags: &TagBook) -> Self {
        let cidr = bucket.subnet.cidr();
        Self {
            country: bucket.country.clone(),
            quantity: bucket.proxies.len(),
            tags: tags
                .get(&cidr)
                .iter()
                .map(|tag| tag.as_str().to_string())
                .collect(),
            cidr,
            health: bucket.health,
            probe: bucket.probe.clone(),
        }
    }

    pub fn list(buckets: &[Bucket], tags: &TagBook) -> Vec<Self> {
        buckets
            .iter()
            .map(|bucket| Self::new(bucket, tags))
            .collect()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub rows: Vec<SubnetRow>,
    pub added: Vec<String>,
    pub grown: Vec<String>,
    pub skipped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunKind {
    Check,
    Probe,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProgress {
    pub kind: RunKind,
    pub done: usize,
    pub total: usize,
    pub eta_seconds: Option<u64>,
    pub row: SubnetRow,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub kind: RunKind,
    pub stopped: bool,
    pub rows: Vec<SubnetRow>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Health;
    use crate::proxy::ProxyLine;
    use crate::subnet::Subnet;
    use crate::tag::Tag;

    fn bucket() -> Bucket {
        let subnet = Subnet::from_host("192.0.2.10".parse().unwrap());
        let proxy = ProxyLine {
            host: "192.0.2.10".parse().unwrap(),
            port: 8080,
            username: "user".into(),
            password: "pass".into(),
            source: "192.0.2.10:8080:user:pass".into(),
        };
        Bucket::new(subnet, vec![proxy], Some("FR".into()))
    }

    fn temp_tags() -> (std::path::PathBuf, TagBook) {
        let dir = std::env::temp_dir().join(format!(
            "proxybench-wire-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let book = TagBook::load(dir.join("tags.json")).unwrap();
        (dir, book)
    }

    #[test]
    fn row_serializes_camel_case_with_nullable_results() {
        let (dir, mut tags) = temp_tags();
        tags.set("192.0.2.0/24".into(), vec![Tag::parse("isp").unwrap()])
            .unwrap();
        let row = SubnetRow::new(&bucket(), &tags);
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "cidr": "192.0.2.0/24",
                "country": "FR",
                "quantity": 1,
                "tags": ["isp"],
                "health": null,
                "probe": null
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn row_carries_health_and_probe() {
        let (dir, tags) = temp_tags();
        let mut checked = bucket();
        checked.health = Some(Health::new(1, 1, 9));
        let json = serde_json::to_value(SubnetRow::new(&checked, &tags)).unwrap();
        assert_eq!(json["health"]["status"], "alive");
        assert_eq!(json["health"]["at"], 9);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_kinds_serialize_lowercase() {
        assert_eq!(serde_json::to_value(RunKind::Check).unwrap(), "check");
        assert_eq!(serde_json::to_value(RunKind::Probe).unwrap(), "probe");
    }
}

use std::fs;
use std::io::ErrorKind;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::secure_file;
use crate::target::Target;

const DEFAULT_TARGET: &str = "https://cp.cloudflare.com/generate_204";
const IN_FLIGHT: RangeInclusive<usize> = 8..=1024;
const STARTS_PER_SECOND: RangeInclusive<usize> = 1..=500;
const TIMEOUT_SECONDS: RangeInclusive<u64> = 1..=30;
const CHECK_SAMPLE: RangeInclusive<usize> = 1..=10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Draft", into = "Draft")]
pub struct Settings {
    pub target: Target,
    pub in_flight: usize,
    pub starts_per_second: usize,
    pub timeout_seconds: u64,
    pub auto_check: bool,
    pub check_sample: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Draft {
    target: String,
    in_flight: usize,
    starts_per_second: usize,
    timeout_seconds: u64,
    auto_check: bool,
    check_sample: usize,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Bounds<T> {
    min: T,
    max: T,
}

impl<T: Copy> From<&RangeInclusive<T>> for Bounds<T> {
    fn from(range: &RangeInclusive<T>) -> Self {
        Self {
            min: *range.start(),
            max: *range.end(),
        }
    }
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    in_flight: Bounds<usize>,
    starts_per_second: Bounds<usize>,
    timeout_seconds: Bounds<u64>,
    check_sample: Bounds<usize>,
}

impl Limits {
    fn new() -> Self {
        Self {
            in_flight: (&IN_FLIGHT).into(),
            starts_per_second: (&STARTS_PER_SECOND).into(),
            timeout_seconds: (&TIMEOUT_SECONDS).into(),
            check_sample: (&CHECK_SAMPLE).into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SettingsSheet {
    current: Settings,
    defaults: Settings,
    limits: Limits,
}

impl SettingsSheet {
    pub fn new(current: Settings) -> Self {
        Self {
            current,
            defaults: Settings::default(),
            limits: Limits::new(),
        }
    }
}

impl Settings {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target: Target::parse(DEFAULT_TARGET).expect("default target is valid"),
            in_flight: 128,
            starts_per_second: 20,
            timeout_seconds: 5,
            auto_check: true,
            check_sample: 2,
        }
    }
}

impl Default for Draft {
    fn default() -> Self {
        Settings::default().into()
    }
}

impl From<Settings> for Draft {
    fn from(settings: Settings) -> Self {
        Self {
            target: settings.target.as_str().to_string(),
            in_flight: settings.in_flight,
            starts_per_second: settings.starts_per_second,
            timeout_seconds: settings.timeout_seconds,
            auto_check: settings.auto_check,
            check_sample: settings.check_sample,
        }
    }
}

impl TryFrom<Draft> for Settings {
    type Error = String;

    fn try_from(draft: Draft) -> Result<Self, String> {
        within("In flight", &IN_FLIGHT, draft.in_flight)?;
        within(
            "Starts per second",
            &STARTS_PER_SECOND,
            draft.starts_per_second,
        )?;
        within("Timeout seconds", &TIMEOUT_SECONDS, draft.timeout_seconds)?;
        within("Check sample", &CHECK_SAMPLE, draft.check_sample)?;
        Ok(Self {
            target: Target::parse(&draft.target)?,
            in_flight: draft.in_flight,
            starts_per_second: draft.starts_per_second,
            timeout_seconds: draft.timeout_seconds,
            auto_check: draft.auto_check,
            check_sample: draft.check_sample,
        })
    }
}

fn within<T: PartialOrd + std::fmt::Display>(
    label: &str,
    range: &RangeInclusive<T>,
    value: T,
) -> Result<(), String> {
    if range.contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "{label} must be between {} and {}.",
            range.start(),
            range.end()
        ))
    }
}

pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Settings, String> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Settings::default()),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(settings).map_err(|err| err.to_string())?;
        secure_file::write_atomic(&self.path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file() -> (PathBuf, SettingsFile) {
        let dir = std::env::temp_dir().join(format!(
            "proxybench-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = SettingsFile::new(dir.join("settings.json"));
        (dir, file)
    }

    fn parse(json: &str) -> Result<Settings, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn defaults_match_the_documented_values() {
        let settings = Settings::default();
        assert_eq!(settings.target.as_str(), DEFAULT_TARGET);
        assert_eq!(settings.in_flight, 128);
        assert_eq!(settings.starts_per_second, 20);
        assert_eq!(settings.timeout_seconds, 5);
        assert!(settings.auto_check);
        assert_eq!(settings.check_sample, 2);
        assert_eq!(settings.timeout(), Duration::from_secs(5));
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let settings = parse(r#"{"inFlight": 64}"#).unwrap();
        assert_eq!(settings.in_flight, 64);
        assert_eq!(settings.check_sample, 2);
        assert_eq!(settings.target.as_str(), DEFAULT_TARGET);
    }

    #[test]
    fn range_edges_are_accepted() {
        assert!(
            parse(r#"{"inFlight":8,"startsPerSecond":1,"timeoutSeconds":1,"checkSample":1}"#)
                .is_ok()
        );
        assert!(parse(
            r#"{"inFlight":1024,"startsPerSecond":500,"timeoutSeconds":30,"checkSample":10}"#
        )
        .is_ok());
    }

    #[test]
    fn out_of_range_values_are_rejected_with_a_message() {
        for json in [
            r#"{"inFlight":7}"#,
            r#"{"inFlight":1025}"#,
            r#"{"startsPerSecond":0}"#,
            r#"{"startsPerSecond":501}"#,
            r#"{"timeoutSeconds":0}"#,
            r#"{"timeoutSeconds":31}"#,
            r#"{"checkSample":0}"#,
            r#"{"checkSample":11}"#,
        ] {
            let err = parse(json).unwrap_err().to_string();
            assert!(err.contains("must be between"), "{json}: {err}");
        }
    }

    #[test]
    fn sheet_limits_match_the_validated_ranges() {
        let sheet = serde_json::to_value(SettingsSheet::new(Settings::default())).unwrap();
        assert_eq!(
            sheet["defaults"],
            serde_json::to_value(Settings::default()).unwrap()
        );
        assert_eq!(sheet["current"], sheet["defaults"]);
        let limits = &sheet["limits"];
        for (key, range) in [
            ("inFlight", (8, 1024)),
            ("startsPerSecond", (1, 500)),
            ("timeoutSeconds", (1, 30)),
            ("checkSample", (1, 10)),
        ] {
            assert_eq!(limits[key]["min"], range.0, "{key}");
            assert_eq!(limits[key]["max"], range.1, "{key}");
            for value in [range.0, range.1] {
                let json = format!(r#"{{"{key}":{value}}}"#);
                assert!(parse(&json).is_ok(), "{json}");
            }
            for value in [range.0 - 1, range.1 + 1] {
                let json = format!(r#"{{"{key}":{value}}}"#);
                assert!(parse(&json).is_err(), "{json}");
            }
        }
    }

    #[test]
    fn invalid_target_is_rejected() {
        assert!(parse(r#"{"target":"http://example.com"}"#).is_err());
        assert!(parse(r#"{"target":"https://user:pass@example.com"}"#).is_err());
        assert!(parse(r#"{"target":""}"#).is_err());
    }

    #[test]
    fn target_is_trimmed() {
        let settings = parse(r#"{"target":"  https://example.com/x  "}"#).unwrap();
        assert_eq!(settings.target.as_str(), "https://example.com/x");
    }

    #[test]
    fn serializes_camel_case_with_target_text() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "target": DEFAULT_TARGET,
                "inFlight": 128,
                "startsPerSecond": 20,
                "timeoutSeconds": 5,
                "autoCheck": true,
                "checkSample": 2
            })
        );
    }

    #[test]
    fn save_then_load_roundtrips() {
        let (dir, file) = temp_file();
        let settings = parse(
            r#"{"target":"https://example.com/ok","inFlight":16,"startsPerSecond":3,"timeoutSeconds":9,"autoCheck":false,"checkSample":4}"#,
        )
        .unwrap();
        file.save(&settings).unwrap();
        assert_eq!(file.load().unwrap(), settings);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_gives_defaults() {
        let (dir, file) = temp_file();
        assert_eq!(file.load().unwrap(), Settings::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_ignores_corrupt_or_invalid_files() {
        let (dir, file) = temp_file();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("settings.json"), "{not json").unwrap();
        assert_eq!(file.load().unwrap(), Settings::default());
        fs::write(dir.join("settings.json"), r#"{"inFlight":1}"#).unwrap();
        assert_eq!(file.load().unwrap(), Settings::default());
        let _ = fs::remove_dir_all(&dir);
    }
}

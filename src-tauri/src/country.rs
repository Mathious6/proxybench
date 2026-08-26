use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::time::Duration;

use crate::split::Subnet;

const ENDPOINT: &str = "https://api.country.is";
const TIMEOUT: Duration = Duration::from_secs(8);

#[derive(serde::Deserialize)]
struct Lookup {
    country: Option<String>,
}

pub fn lookup(ips: &[Ipv4Addr]) -> HashMap<String, String> {
    let mut countries = HashMap::new();
    for ip in ips {
        if let Some(row) = fetch(*ip) {
            if let Some(code) = normalize(row.country.as_deref()) {
                countries.insert(Subnet::from_host(*ip).cidr(), code);
            }
        }
    }
    countries
}

fn fetch(ip: Ipv4Addr) -> Option<Lookup> {
    ureq::get(&format!("{ENDPOINT}/{ip}"))
        .timeout(TIMEOUT)
        .call()
        .ok()?
        .into_json()
        .ok()
}

fn normalize(value: Option<&str>) -> Option<String> {
    let code = value?;
    if code.len() == 2 && code.bytes().all(|b| b.is_ascii_alphabetic()) {
        Some(code.to_ascii_uppercase())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_uppercases_iso_alpha2() {
        assert_eq!(normalize(Some("fr")), Some("FR".into()));
        assert_eq!(normalize(Some("US")), Some("US".into()));
    }

    #[test]
    fn normalize_rejects_empty_and_non_iso() {
        assert_eq!(normalize(None), None);
        assert_eq!(normalize(Some("")), None);
        assert_eq!(normalize(Some("FRA")), None);
        assert_eq!(normalize(Some("12")), None);
    }
}

use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub origin_form: String,
}

impl Target {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let url = Url::parse(raw).map_err(|_| "Enter an HTTPS URL.".to_string())?;
        if url.scheme() != "https" {
            return Err("Enter an HTTPS URL.".into());
        }
        if url.authority().contains('@') {
            return Err("The target URL must not include credentials.".into());
        }
        let host = url
            .host_str()
            .filter(|host| !host.is_empty())
            .ok_or_else(|| "Enter an HTTPS URL.".to_string())?
            .to_string();
        let port = url.port().unwrap_or(443);
        let origin_form = origin_form(&url);
        Ok(Self {
            host,
            port,
            origin_form,
        })
    }

    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn host_header(&self) -> String {
        if self.port == 443 {
            self.host.clone()
        } else {
            self.authority()
        }
    }
}

fn origin_form(url: &Url) -> String {
    let mut path = url.path().to_string();
    if path.is_empty() {
        path = "/".into();
    }
    match url.query() {
        Some(query) => format!("{path}?{query}"),
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_https_defaults_port_and_root_path() {
        let target = Target::parse("https://example.com").unwrap();
        assert_eq!(target.host, "example.com");
        assert_eq!(target.port, 443);
        assert_eq!(target.origin_form, "/");
        assert_eq!(target.authority(), "example.com:443");
        assert_eq!(target.host_header(), "example.com");
    }

    #[test]
    fn parse_keeps_path_query_and_custom_port() {
        let target = Target::parse("https://example.com:8443/status?probe=1#frag").unwrap();
        assert_eq!(target.port, 8443);
        assert_eq!(target.origin_form, "/status?probe=1");
        assert_eq!(target.host_header(), "example.com:8443");
    }

    #[test]
    fn parse_rejects_non_https_and_userinfo() {
        assert!(Target::parse("http://example.com").is_err());
        assert!(Target::parse("ftp://example.com").is_err());
        assert!(Target::parse("https://user:pass@example.com").is_err());
        assert!(Target::parse("https://user@example.com").is_err());
        assert!(Target::parse("https://:pass@example.com").is_err());
        assert!(Target::parse("not a url").is_err());
        assert!(Target::parse("https://").is_err());
    }

    #[test]
    fn parse_treats_empty_userinfo_as_absent() {
        let target = Target::parse("https://@example.com").unwrap();
        assert_eq!(target.host, "example.com");
    }
}

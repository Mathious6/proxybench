use std::net::Ipv4Addr;

#[derive(Clone, PartialEq, Eq)]
pub struct ProxyLine {
    pub host: Ipv4Addr,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub source: String,
}

pub fn parse_text(text: &str) -> (Vec<ProxyLine>, usize) {
    let mut proxies = Vec::new();
    let mut skipped = 0;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_line(line) {
            Some(mut proxy) => {
                proxy.source = raw.to_string();
                proxies.push(proxy);
            }
            None => skipped += 1,
        }
    }
    (proxies, skipped)
}

fn parse_line(line: &str) -> Option<ProxyLine> {
    let mut parts = line.splitn(4, ':');
    let host = parts.next()?.parse().ok()?;
    let port = parts.next()?.parse().ok()?;
    let username = parts.next()?.to_string();
    let password = parts.next()?.to_string();
    if port == 0 || username.is_empty() || password.is_empty() {
        return None;
    }
    Some(ProxyLine {
        host,
        port,
        username,
        password,
        source: line.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line_accepts_four_fields() {
        let proxy = parse_line("192.0.2.10:8080:user:pass").unwrap();
        assert_eq!(proxy.host, Ipv4Addr::new(192, 0, 2, 10));
        assert_eq!(proxy.port, 8080);
        assert_eq!(proxy.username, "user");
        assert_eq!(proxy.password, "pass");
        assert_eq!(proxy.source, "192.0.2.10:8080:user:pass");
    }

    #[test]
    fn parse_line_keeps_colons_in_password() {
        let proxy = parse_line("192.0.2.10:8080:user:p:ss").unwrap();
        assert_eq!(proxy.password, "p:ss");
    }

    #[test]
    fn parse_line_rejects_hostname_and_socks() {
        assert!(parse_line("proxy.example:8080:user:pass").is_none());
        assert!(parse_line("socks5://192.0.2.10:1080").is_none());
        assert!(parse_line("192.0.2.10:8080").is_none());
        assert!(parse_line("192.0.2.10:0:user:pass").is_none());
    }

    #[test]
    fn parse_text_skips_blank_comment_and_bad_lines() {
        let text = "\n# pool\n192.0.2.10:8080:user:pass\nbad\n192.0.2.11:8080:user:pass\n";
        let (proxies, skipped) = parse_text(text);
        assert_eq!(proxies.len(), 2);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn parse_text_keeps_accepted_source_line_verbatim() {
        let text = "  192.0.2.10:8080:user:p:ss  \n";
        let (proxies, skipped) = parse_text(text);
        assert_eq!(skipped, 0);
        assert_eq!(proxies[0].source, "  192.0.2.10:8080:user:p:ss  ");
        assert_eq!(proxies[0].password, "p:ss");
    }
}

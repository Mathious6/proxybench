use std::collections::BTreeMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PUBLIC_RESOLVERS: [&str; 2] = ["1.1.1.1:53", "8.8.8.8:53"];
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);
const CACHE_TTL: Duration = Duration::from_secs(300);
const TYPE_A: u16 = 1;

fn cache() -> &'static Mutex<BTreeMap<String, CachedAddresses>> {
    static CACHE: Mutex<BTreeMap<String, CachedAddresses>> = Mutex::new(BTreeMap::new());
    &CACHE
}

struct CachedAddresses {
    at: Instant,
    addresses: Vec<SocketAddr>,
}

pub fn resolve(netloc: &str) -> io::Result<Vec<SocketAddr>> {
    let (host, port) = split_netloc(netloc)?;
    if let Some(cached) = cached(&host) {
        return Ok(cached);
    }
    let usable = filter_usable(system_addresses(&host, port));
    let addresses = if usable.is_empty() {
        query_public(&host, port)
    } else {
        usable
    };
    if addresses.is_empty() {
        Err(io::Error::other(format!(
            "dns resolution failed for {host}"
        )))
    } else {
        remember(&host, &addresses);
        Ok(addresses)
    }
}

fn cached(host: &str) -> Option<Vec<SocketAddr>> {
    let cache = cache().lock().ok()?;
    let cached = cache.get(host)?;
    if cached.at.elapsed() < CACHE_TTL {
        Some(cached.addresses.clone())
    } else {
        None
    }
}

fn remember(host: &str, addresses: &[SocketAddr]) {
    if let Ok(mut cache) = cache().lock() {
        cache.insert(
            host.to_string(),
            CachedAddresses {
                at: Instant::now(),
                addresses: addresses.to_vec(),
            },
        );
    }
}

fn split_netloc(netloc: &str) -> io::Result<(String, u16)> {
    let (host, port) = netloc
        .rsplit_once(':')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "netloc without port"))?;
    let port = port
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid port"))?;
    Ok((host.to_string(), port))
}

fn system_addresses(host: &str, port: u16) -> Vec<SocketAddr> {
    format!("{host}:{port}")
        .to_socket_addrs()
        .map(|addrs| addrs.collect())
        .unwrap_or_default()
}

fn filter_usable(addresses: Vec<SocketAddr>) -> Vec<SocketAddr> {
    addresses
        .into_iter()
        .filter(|address| !address.ip().is_unspecified())
        .collect()
}

fn query_public(host: &str, port: u16) -> Vec<SocketAddr> {
    PUBLIC_RESOLVERS
        .iter()
        .filter_map(|resolver| {
            query_a_records(host, resolver).map(|records| {
                records
                    .into_iter()
                    .map(|ip| SocketAddr::new(ip, port))
                    .collect::<Vec<_>>()
            })
        })
        .next()
        .unwrap_or_default()
}

fn query_a_records(host: &str, resolver: &str) -> Option<Vec<IpAddr>> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(RESPONSE_TIMEOUT)).ok()?;
    let query = build_query(host)?;
    socket.send_to(&query, resolver).ok()?;
    let mut buffer = [0u8; 1024];
    let received = socket.recv(&mut buffer).ok()?;
    parse_a_records(&buffer[..received]).filter(|records| !records.is_empty())
}

fn build_query(host: &str) -> Option<Vec<u8>> {
    let mut query = vec![0x53, 0x1f, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    for label in host.split('.') {
        let length = label.len();
        if length == 0 || length > 63 {
            return None;
        }
        query.push(length as u8);
        query.extend_from_slice(label.as_bytes());
    }
    query.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x01]);
    Some(query)
}

fn parse_a_records(packet: &[u8]) -> Option<Vec<IpAddr>> {
    if packet.len() < 12 {
        return None;
    }
    let answers = u16::from_be_bytes([packet[6], packet[7]]) as usize;
    let mut offset = 12;
    while offset < packet.len() && packet[offset] != 0 {
        offset += packet[offset] as usize + 1;
    }
    offset += 5;
    let mut records = Vec::new();
    for _ in 0..answers {
        offset = skip_name(packet, offset)?;
        if offset + 10 > packet.len() {
            return None;
        }
        let record_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let record_length = u16::from_be_bytes([packet[offset + 8], packet[offset + 9]]) as usize;
        let data_start = offset + 10;
        if data_start + record_length > packet.len() {
            return None;
        }
        if record_type == TYPE_A && record_length == 4 {
            records.push(IpAddr::V4(Ipv4Addr::new(
                packet[data_start],
                packet[data_start + 1],
                packet[data_start + 2],
                packet[data_start + 3],
            )));
        }
        offset = data_start + record_length;
    }
    Some(records)
}

fn skip_name(packet: &[u8], mut offset: usize) -> Option<usize> {
    loop {
        let byte = *packet.get(offset)?;
        if byte & 0xC0 == 0xC0 {
            return Some(offset + 2);
        }
        if byte == 0 {
            return Some(offset + 1);
        }
        offset += byte as usize + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trips_addresses() {
        let host = "cache.test";
        assert!(cached(host).is_none());
        remember(
            host,
            &[SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 443)],
        );
        assert_eq!(
            cached(host).unwrap(),
            vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 443)]
        );
    }

    #[test]
    fn build_query_encodes_a_record_lookup() {
        let query = build_query("api.country.is").unwrap();
        assert_eq!(&query[0..2], &[0x53, 0x1f]);
        assert_eq!(&query[2..4], &[0x01, 0x00]);
        assert_eq!(
            &query[10..],
            &[
                3, b'a', b'p', b'i', 7, b'c', b'o', b'u', b'n', b't', b'r', b'y', 2, b'i', b's', 0,
                0, 1, 0, 1
            ]
        );
    }

    #[test]
    fn build_query_rejects_empty_labels() {
        assert!(build_query("").is_none());
    }

    #[test]
    fn parse_a_records_skips_cname_and_reads_addresses() {
        let mut packet = vec![
            0x53, 0x1f, 0x81, 0x80, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
        ];
        for label in "api.country.is".split('.') {
            packet.push(label.len() as u8);
            packet.extend_from_slice(label.as_bytes());
        }
        packet.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x01]);
        packet.extend_from_slice(&[
            0xC0, 0x0C, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x7B, 0x00, 0x00,
        ]);
        packet.extend_from_slice(&[
            0xC0, 0x0C, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x7B, 0x00, 0x04, 1, 2, 3, 4,
        ]);
        let records = parse_a_records(&packet).unwrap();
        assert_eq!(records, vec![IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))]);
    }

    #[test]
    fn parse_a_records_rejects_truncated_packets() {
        assert!(parse_a_records(&[0x53, 0x1f]).is_none());
        assert!(parse_a_records(&[]).is_none());
    }

    #[test]
    fn filter_usable_drops_unspecified_addresses() {
        let addresses = vec![
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 443),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 443),
            SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), 443),
        ];
        let usable = filter_usable(addresses);
        assert_eq!(usable.len(), 1);
        assert_eq!(usable[0].ip(), IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[test]
    fn split_netloc_parses_host_and_port() {
        let (host, port) = split_netloc("api.country.is:443").unwrap();
        assert_eq!(host, "api.country.is");
        assert_eq!(port, 443);
        assert!(split_netloc("no-port").is_err());
    }
}

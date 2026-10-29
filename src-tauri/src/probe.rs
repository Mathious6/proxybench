use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use bytes::Bytes;
use http_body_util::Empty;
use hyper::body::Incoming;
use hyper::header::{HeaderValue, HOST};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::proxy::ProxyLine;
use crate::target::Target;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub connect: Duration,
    pub ttfb: Duration,
}

impl Sample {
    fn connect_only(connect: Duration) -> Self {
        Self {
            connect,
            ttfb: connect,
        }
    }
}

pub fn connector() -> TlsConnector {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    connector_with_roots(roots)
}

pub async fn measure(
    proxy: &ProxyLine,
    target: &Target,
    tls: &TlsConnector,
    deadline: Duration,
) -> Option<Sample> {
    tokio::time::timeout(deadline, measure_inner(proxy, target, tls))
        .await
        .ok()?
}

pub async fn check(proxy: &ProxyLine, target: &Target, deadline: Duration) -> Option<Sample> {
    tokio::time::timeout(deadline, check_inner(proxy, target))
        .await
        .ok()?
}

fn connector_with_roots(roots: rustls::RootCertStore) -> TlsConnector {
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.resumption = rustls::client::Resumption::disabled();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    TlsConnector::from(Arc::new(config))
}

async fn open_tunnel(
    proxy: &ProxyLine,
    target: &Target,
    started: Instant,
) -> Option<(Response<Incoming>, Duration)> {
    let addr = SocketAddr::from((proxy.host, proxy.port));
    let stream = TcpStream::connect(addr).await.ok()?;
    let _ = stream.set_nodelay(true);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .ok()?;
    tokio::spawn(async move {
        let _ = conn.with_upgrades().await;
    });
    let auth = HeaderValue::from_str(&format!(
        "Basic {}",
        BASE64.encode(format!("{}:{}", proxy.username, proxy.password))
    ))
    .ok()?;
    let connect = Request::builder()
        .method(Method::CONNECT)
        .uri(target.authority())
        .header(HOST, target.authority())
        .header("Proxy-Authorization", auth)
        .body(Empty::<Bytes>::new())
        .ok()?;
    let response = sender.send_request(connect).await.ok()?;
    if response.status() != StatusCode::OK {
        return None;
    }
    Some((response, started.elapsed()))
}

async fn check_inner(proxy: &ProxyLine, target: &Target) -> Option<Sample> {
    let (_response, connect) = open_tunnel(proxy, target, Instant::now()).await?;
    Some(Sample::connect_only(connect))
}

async fn measure_inner(proxy: &ProxyLine, target: &Target, tls: &TlsConnector) -> Option<Sample> {
    let started = Instant::now();
    let (response, connect) = open_tunnel(proxy, target, started).await?;
    let upgraded = hyper::upgrade::on(response).await.ok()?;
    let server_name = ServerName::try_from(target.host.clone()).ok()?;
    let tls_stream = tls
        .connect(server_name, TokioIo::new(upgraded))
        .await
        .ok()?;
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(tls_stream))
        .await
        .ok()?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let request = Request::builder()
        .method(Method::GET)
        .uri(&target.origin_form)
        .header(HOST, target.host_header())
        .header("Connection", "close")
        .body(Empty::<Bytes>::new())
        .ok()?;
    let response = sender.send_request(request).await.ok()?;
    let ttfb = started.elapsed();
    drop(response);
    Some(Sample { connect, ttfb })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    fn proxy(port: u16) -> ProxyLine {
        ProxyLine {
            host: Ipv4Addr::LOCALHOST,
            port,
            username: "user".into(),
            password: "pass".into(),
            source: format!("127.0.0.1:{port}:user:pass"),
        }
    }

    fn identity() -> (
        CertificateDer<'static>,
        PrivateKeyDer<'static>,
        TlsConnector,
    ) {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = CertificateDer::from(certified.cert.der().to_vec());
        let key =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert.clone()).unwrap();
        (cert, key, connector_with_roots(roots))
    }

    async fn serve(
        listener: TcpListener,
        cert: CertificateDer<'static>,
        key: PrivateKeyDer<'static>,
        status: &'static [u8],
    ) {
        let (mut stream, _) = listener.accept().await.unwrap();
        let headers = read_headers(&mut stream).await;
        let text = String::from_utf8_lossy(&headers).to_ascii_lowercase();
        assert!(text.starts_with("connect localhost:443 "));
        assert!(text.contains("proxy-authorization: basic "));
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
        let mut server = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap();
        server.alpn_protocols = vec![b"http/1.1".to_vec()];
        let mut tls = TlsAcceptor::from(Arc::new(server))
            .accept(stream)
            .await
            .unwrap();
        let _ = read_headers(&mut tls).await;
        tls.write_all(status).await.unwrap();
        tls.flush().await.unwrap();
    }

    async fn read_headers<S: AsyncReadExt + Unpin>(stream: &mut S) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).await.unwrap();
            buf.push(byte[0]);
            if buf.len() >= 4 && buf[buf.len() - 4..] == *b"\r\n\r\n" {
                return buf;
            }
            if buf.len() > 16 * 1024 {
                panic!("headers too large");
            }
        }
    }

    const LONG: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn measure_times_connect_before_tls_and_accepts_any_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (cert, key, tls) = identity();
        let server = tokio::spawn(serve(
            listener,
            cert,
            key,
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 4\r\n\r\nnope",
        ));
        let sample = measure(
            &proxy(port),
            &Target::parse("https://localhost/status").unwrap(),
            &tls,
            LONG,
        )
        .await
        .unwrap();
        assert!(sample.connect <= sample.ttfb);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn measure_fails_when_connect_is_not_200() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(reply_once(
            listener,
            b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n",
        ));
        assert!(measure(
            &proxy(port),
            &Target::parse("https://localhost/").unwrap(),
            &connector(),
            LONG,
        )
        .await
        .is_none());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn measure_fails_after_the_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let _stream = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let started = Instant::now();
        assert!(measure(
            &proxy(port),
            &Target::parse("https://localhost/").unwrap(),
            &connector(),
            Duration::from_millis(200),
        )
        .await
        .is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
        server.abort();
    }

    async fn reply_once(listener: TcpListener, response: &'static [u8]) {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_headers(&mut stream).await;
        stream.write_all(response).await.unwrap();
    }

    #[tokio::test]
    async fn check_succeeds_on_connect_200_and_closes_without_tls() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let headers = read_headers(&mut stream).await;
            let text = String::from_utf8_lossy(&headers).to_ascii_lowercase();
            assert!(text.starts_with("connect localhost:443 "));
            assert!(text.contains("proxy-authorization: basic "));
            stream
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            let mut rest = Vec::new();
            tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut rest))
                .await
                .expect("check closed the tunnel")
                .unwrap();
            assert!(rest.is_empty());
        });
        let sample = check(
            &proxy(port),
            &Target::parse("https://localhost/").unwrap(),
            LONG,
        )
        .await
        .unwrap();
        assert_eq!(sample.connect, sample.ttfb);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn check_fails_when_connect_is_not_200() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(reply_once(listener, b"HTTP/1.1 502 Bad Gateway\r\n\r\n"));
        assert!(check(
            &proxy(port),
            &Target::parse("https://localhost/").unwrap(),
            LONG,
        )
        .await
        .is_none());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn check_fails_when_nothing_listens() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(check(
            &proxy(port),
            &Target::parse("https://localhost/").unwrap(),
            LONG,
        )
        .await
        .is_none());
    }

    #[tokio::test]
    async fn check_fails_after_the_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let _stream = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        assert!(check(
            &proxy(port),
            &Target::parse("https://localhost/").unwrap(),
            Duration::from_millis(200),
        )
        .await
        .is_none());
        server.abort();
    }
}

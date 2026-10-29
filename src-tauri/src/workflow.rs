use crate::clock;
use crate::country;
use crate::inventory::Bucket;
use crate::probe;
use crate::run::{self, Ending, Limits, Progress, Workload};
use crate::sample;
use crate::slot::RunGuard;
use crate::state::AppState;
use crate::wire::{RunKind, RunProgress, RunSummary, SubnetRow};

pub async fn check<Emit>(
    state: &AppState,
    cidrs: Option<Vec<String>>,
    emit: Emit,
) -> Result<RunSummary, String>
where
    Emit: Fn(RunProgress) + Send + Sync,
{
    let guard = state.slot.begin_check()?;
    let settings = state.settings()?;
    let buckets = state.inventory()?.scope(cidrs)?;
    let workloads = buckets
        .iter()
        .map(|bucket| Workload {
            subnet: bucket.subnet,
            proxies: sample::evenly(&bucket.proxies, settings.check_sample),
        })
        .collect();
    let target = settings.target.clone();
    let timeout = settings.timeout();
    let ending = run::schedule(
        workloads,
        Limits::from(&settings),
        guard.cancel.clone(),
        move |proxy| {
            let target = target.clone();
            async move { probe::check(&proxy, &target, timeout).await }
        },
        |progress| record(state, RunKind::Check, progress, &emit),
    )
    .await;
    summarize(state, RunKind::Check, &buckets, ending)
}

pub async fn probe<Emit>(
    state: &AppState,
    cidrs: Option<Vec<String>>,
    emit: Emit,
) -> Result<RunSummary, String>
where
    Emit: Fn(RunProgress) + Send + Sync,
{
    let guard = state.slot.begin_probe().await?;
    let settings = state.settings()?;
    let buckets = state.inventory()?.scope(cidrs)?;
    if buckets.is_empty() {
        return Err("Import proxies before probing.".into());
    }
    if !refresh_countries(state, &buckets, &guard).await? {
        return summarize(state, RunKind::Probe, &buckets, Ending::Stopped);
    }
    let workloads = buckets
        .iter()
        .map(|bucket| Workload {
            subnet: bucket.subnet,
            proxies: bucket.proxies.clone(),
        })
        .collect();
    let target = settings.target.clone();
    let timeout = settings.timeout();
    let tls = probe::connector();
    let ending = run::schedule(
        workloads,
        Limits::from(&settings),
        guard.cancel.clone(),
        move |proxy| {
            let target = target.clone();
            let tls = tls.clone();
            async move { probe::measure(&proxy, &target, &tls, timeout).await }
        },
        |progress| record(state, RunKind::Probe, progress, &emit),
    )
    .await;
    summarize(state, RunKind::Probe, &buckets, ending)
}

async fn refresh_countries(
    state: &AppState,
    buckets: &[Bucket],
    guard: &RunGuard<'_>,
) -> Result<bool, String> {
    let hosts: Vec<_> = buckets
        .iter()
        .filter_map(|bucket| bucket.proxies.first().map(|proxy| proxy.host))
        .collect();
    let lookup = tauri::async_runtime::spawn_blocking(move || country::lookup(&hosts));
    let countries = tokio::select! {
        _ = guard.cancel.cancelled() => return Ok(false),
        found = lookup => found.map_err(|err| err.to_string())?,
    };
    state.update_inventory(|inventory| inventory.record_countries(&countries))?;
    Ok(true)
}

fn record<Emit>(state: &AppState, kind: RunKind, progress: Progress, emit: &Emit)
where
    Emit: Fn(RunProgress),
{
    let Some(row) = apply(state, kind, &progress) else {
        return;
    };
    emit(RunProgress {
        kind,
        done: progress.done,
        total: progress.total,
        eta_seconds: progress.eta_seconds,
        row,
    });
}

fn apply(state: &AppState, kind: RunKind, progress: &Progress) -> Option<SubnetRow> {
    let at = clock::now_ms();
    let mut inventory = state.inventory().ok()?;
    match kind {
        RunKind::Check => inventory.record_check(progress.subnet, progress.tally.health(at)),
        RunKind::Probe => inventory.record_probe(progress.subnet, progress.tally.probe(at)),
    }
    let tags = state.tags().ok()?;
    inventory
        .get(progress.subnet)
        .map(|bucket| SubnetRow::new(bucket, &tags))
}

fn summarize(
    state: &AppState,
    kind: RunKind,
    scope: &[Bucket],
    ending: Ending,
) -> Result<RunSummary, String> {
    state.persist_inventory()?;
    let inventory = state.inventory()?;
    let tags = state.tags()?;
    let rows = scope
        .iter()
        .filter_map(|bucket| inventory.get(bucket.subnet))
        .map(|bucket| SubnetRow::new(bucket, &tags))
        .collect();
    Ok(RunSummary {
        kind,
        stopped: ending == Ending::Stopped,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::sync::Mutex;
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use crate::health::Status;
    use crate::proxy::ProxyLine;
    use crate::settings::Settings;
    use crate::subnet::Group;
    use crate::subnet::Subnet;

    fn temp_state() -> (std::path::PathBuf, AppState) {
        let dir = std::env::temp_dir().join(format!(
            "proxybench-workflow-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state = AppState::open(&dir).unwrap();
        let settings: Settings = serde_json::from_str(
            r#"{"target":"https://localhost/","timeoutSeconds":2,"checkSample":2}"#,
        )
        .unwrap();
        state.replace_settings(settings).unwrap();
        (dir, state)
    }

    fn local_proxy(port: u16) -> ProxyLine {
        ProxyLine {
            host: Ipv4Addr::LOCALHOST,
            port,
            username: "user".into(),
            password: "pass".into(),
            source: format!("127.0.0.1:{port}:user:pass"),
        }
    }

    fn stock(state: &AppState, ports: &[u16]) {
        let group = Group {
            subnet: Subnet::from_host(Ipv4Addr::LOCALHOST),
            proxies: ports.iter().map(|port| local_proxy(*port)).collect(),
        };
        state
            .update_inventory(|inventory| {
                inventory.merge(vec![group], &std::collections::HashMap::new())
            })
            .unwrap();
    }

    async fn serve_connect(listener: TcpListener, reply: &'static [u8]) {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut seen = Vec::new();
                let mut byte = [0u8; 1];
                while !seen.ends_with(b"\r\n\r\n") {
                    if stream.read_exact(&mut byte).await.is_err() {
                        return;
                    }
                    seen.push(byte[0]);
                }
                let _ = stream.write_all(reply).await;
                let mut rest = Vec::new();
                let _ = stream.read_to_end(&mut rest).await;
            });
        }
    }

    async fn proxy_port(reply: &'static [u8]) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(serve_connect(listener, reply));
        port
    }

    #[tokio::test]
    async fn check_marks_a_subnet_alive_and_emits_each_updated_row() {
        let (dir, state) = temp_state();
        let ports = [
            proxy_port(b"HTTP/1.1 200 Connection Established\r\n\r\n").await,
            proxy_port(b"HTTP/1.1 200 Connection Established\r\n\r\n").await,
            proxy_port(b"HTTP/1.1 200 Connection Established\r\n\r\n").await,
        ];
        stock(&state, &ports);
        let events = Mutex::new(Vec::new());
        let summary = check(&state, None, |progress| {
            events.lock().unwrap().push(progress)
        })
        .await
        .unwrap();
        assert!(!summary.stopped);
        assert_eq!(summary.kind, RunKind::Check);
        assert_eq!(summary.rows.len(), 1);
        let health = summary.rows[0].health.unwrap();
        assert_eq!(
            (health.status, health.ok, health.tested),
            (Status::Alive, 2, 2)
        );
        assert!(summary.rows[0].probe.is_none());
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].row.health.unwrap().tested, 1);
        assert_eq!((events[1].done, events[1].total), (2, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn check_marks_a_subnet_dead_when_connect_is_refused() {
        let (dir, state) = temp_state();
        let port = proxy_port(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n").await;
        stock(&state, &[port]);
        let summary = check(&state, None, |_| {}).await.unwrap();
        assert_eq!(summary.rows[0].health.unwrap().status, Status::Dead);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn stop_ends_a_check_promptly_and_keeps_previous_results() {
        let (dir, state) = temp_state();
        let silent = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = silent.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                held.push(silent.accept().await.unwrap());
            }
        });
        stock(&state, &[port]);
        let subnet = Subnet::from_host(Ipv4Addr::LOCALHOST);
        let earlier = crate::health::Health::new(1, 1, 5);
        state
            .update_inventory(|inventory| inventory.record_check(subnet, earlier))
            .unwrap();
        let stopper = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            state.slot.stop();
        };
        let started = std::time::Instant::now();
        let (summary, _) = tokio::join!(check(&state, None, |_| {}), stopper);
        let summary = summary.unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(summary.stopped);
        assert_eq!(summary.rows[0].health, Some(earlier));
        assert!(state.slot.begin_check().is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn check_on_an_empty_inventory_returns_no_rows() {
        let (dir, state) = temp_state();
        let summary = check(&state, None, |_| {}).await.unwrap();
        assert!(summary.rows.is_empty());
        assert!(!summary.stopped);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn probe_without_proxies_asks_for_an_import() {
        let (dir, state) = temp_state();
        let err = probe(&state, None, |_| {}).await.err().unwrap();
        assert_eq!(err, "Import proxies before probing.");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

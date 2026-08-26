use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{Mutex, Semaphore};

use crate::parse::ProxyLine;
use crate::probe::{self, Sample};
use crate::session::StoredBucket;
use crate::stats::{self, milliseconds};
use crate::target::Target;

pub const STARTS_PER_SECOND: usize = 5;
pub const IN_FLIGHT: usize = 32;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub cidr: String,
    pub tested: usize,
    pub ok: usize,
    pub connect_p50: Option<f64>,
    pub connect_p95: Option<f64>,
    pub ttfb_p50: Option<f64>,
    pub ttfb_p95: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub eta_seconds: Option<u64>,
    pub metrics: Metrics,
}

struct BucketState {
    cidr: String,
    tested: usize,
    ok: usize,
    connect: Vec<Duration>,
    ttfb: Vec<Duration>,
}

impl BucketState {
    fn new(cidr: String) -> Self {
        Self {
            cidr,
            tested: 0,
            ok: 0,
            connect: Vec::new(),
            ttfb: Vec::new(),
        }
    }

    fn record(&mut self, sample: Option<Sample>) -> Metrics {
        self.tested += 1;
        if let Some(sample) = sample {
            self.ok += 1;
            self.connect.push(sample.connect);
            self.ttfb.push(sample.ttfb);
        }
        self.metrics()
    }

    fn metrics(&self) -> Metrics {
        let connect = stats::percentiles(&self.connect);
        let ttfb = stats::percentiles(&self.ttfb);
        Metrics {
            cidr: self.cidr.clone(),
            tested: self.tested,
            ok: self.ok,
            connect_p50: connect.map(|value| milliseconds(value.p50)),
            connect_p95: connect.map(|value| milliseconds(value.p95)),
            ttfb_p50: ttfb.map(|value| milliseconds(value.p50)),
            ttfb_p95: ttfb.map(|value| milliseconds(value.p95)),
        }
    }
}

pub async fn schedule<F, Fut, OnProgress>(
    buckets: Vec<StoredBucket>,
    probe: F,
    on_progress: OnProgress,
) -> HashMap<String, Metrics>
where
    F: Fn(ProxyLine) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Sample, ()>> + Send,
    OnProgress: FnMut(Progress) + Send,
{
    schedule_with(buckets, probe, STARTS_PER_SECOND, IN_FLIGHT, on_progress).await
}

async fn schedule_with<F, Fut, OnProgress>(
    buckets: Vec<StoredBucket>,
    probe: F,
    starts_per_second: usize,
    in_flight: usize,
    mut on_progress: OnProgress,
) -> HashMap<String, Metrics>
where
    F: Fn(ProxyLine) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Sample, ()>> + Send,
    OnProgress: FnMut(Progress) + Send,
{
    let total: usize = buckets.iter().map(|bucket| bucket.proxies.len()).sum();
    if total == 0 {
        return HashMap::new();
    }
    let work = round_robin(buckets);
    let mut states = initial_states(&work);
    let jobs = work.len();
    let started = Instant::now();
    let limiter = Arc::new(RateLimit::new(starts_per_second));
    let inflight = Arc::new(Semaphore::new(in_flight));
    let probe = Arc::new(probe);
    let (tx, mut rx) = tokio::sync::mpsc::channel(jobs.max(1));
    let dispatcher = tokio::spawn(async move {
        for (cidr, proxy) in work {
            let permit = inflight.clone().acquire_owned().await.expect("semaphore");
            limiter.acquire().await;
            let probe = probe.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let sample = probe(proxy).await;
                drop(permit);
                let _ = tx.send((cidr, sample)).await;
            });
        }
    });
    let mut done = 0;
    while let Some((cidr, sample)) = rx.recv().await {
        done += 1;
        let metrics = states
            .get_mut(&cidr)
            .expect("scheduled cidr")
            .record(sample.ok());
        let elapsed = started.elapsed();
        on_progress(Progress {
            done,
            total,
            eta_seconds: eta(elapsed, done, total),
            metrics,
        });
    }
    let _ = dispatcher.await;
    states
        .into_iter()
        .map(|(cidr, state)| (cidr, state.metrics()))
        .collect()
}

pub async fn probe_session<OnProgress>(
    buckets: Vec<StoredBucket>,
    target: Target,
    on_progress: OnProgress,
) -> HashMap<String, Metrics>
where
    OnProgress: FnMut(Progress) + Send,
{
    let tls = probe::connector();
    schedule(
        buckets,
        move |proxy| {
            let target = target.clone();
            let tls = tls.clone();
            async move { probe::measure(&proxy, &target, &tls).await }
        },
        on_progress,
    )
    .await
}

fn round_robin(buckets: Vec<StoredBucket>) -> Vec<(String, ProxyLine)> {
    let mut queues: Vec<(String, std::collections::VecDeque<ProxyLine>)> = buckets
        .into_iter()
        .map(|bucket| (bucket.subnet.cidr(), bucket.proxies.into_iter().collect()))
        .collect();
    let mut work = Vec::new();
    loop {
        let mut progressed = false;
        for (cidr, queue) in &mut queues {
            if let Some(proxy) = queue.pop_front() {
                work.push((cidr.clone(), proxy));
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    work
}

fn initial_states(work: &[(String, ProxyLine)]) -> HashMap<String, BucketState> {
    let mut states = HashMap::new();
    for (cidr, _) in work {
        states
            .entry(cidr.clone())
            .or_insert_with(|| BucketState::new(cidr.clone()));
    }
    states
}

fn eta(elapsed: Duration, done: usize, total: usize) -> Option<u64> {
    if done == 0 || done >= total {
        return None;
    }
    let remaining = total - done;
    let per = elapsed / done as u32;
    Some((per * remaining as u32).as_secs())
}

struct RateLimit {
    interval: Duration,
    next: Mutex<Instant>,
}

impl RateLimit {
    fn new(per_second: usize) -> Self {
        Self {
            interval: Duration::from_secs(1) / per_second.max(1) as u32,
            next: Mutex::new(Instant::now()),
        }
    }

    async fn acquire(&self) {
        let wait = {
            let mut next = self.next.lock().await;
            let now = Instant::now();
            let wait = next.saturating_duration_since(now);
            *next = (*next).max(now) + self.interval;
            wait
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ProxyLine;
    use crate::split::Subnet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    fn proxy(host: &str, label: &str) -> ProxyLine {
        ProxyLine {
            host: host.parse().unwrap(),
            port: 8080,
            username: "user".into(),
            password: "pass".into(),
            source: format!("{host}:8080:user:{label}"),
        }
    }

    fn bucket(host: &str, proxies: Vec<ProxyLine>) -> StoredBucket {
        StoredBucket {
            subnet: Subnet::from_host(host.parse().unwrap()),
            proxies,
            country: None,
            last_probe: None,
        }
    }

    fn ok_sample() -> Sample {
        Sample {
            connect: Duration::from_millis(10),
            ttfb: Duration::from_millis(20),
        }
    }

    #[test]
    fn round_robin_interleaves_subnets() {
        let work = round_robin(vec![
            bucket(
                "192.0.2.1",
                vec![proxy("192.0.2.1", "a1"), proxy("192.0.2.2", "a2")],
            ),
            bucket("198.51.100.1", vec![proxy("198.51.100.1", "b1")]),
        ]);
        let labels: Vec<_> = work
            .iter()
            .map(|(_, proxy)| proxy.source.rsplit(':').next().unwrap())
            .collect();
        assert_eq!(labels, vec!["a1", "b1", "a2"]);
    }

    #[tokio::test]
    async fn schedule_starts_in_round_robin_order() {
        let started = Arc::new(StdMutex::new(Vec::new()));
        let observed = started.clone();
        schedule_with(
            vec![
                bucket(
                    "192.0.2.1",
                    vec![proxy("192.0.2.1", "a1"), proxy("192.0.2.2", "a2")],
                ),
                bucket("198.51.100.1", vec![proxy("198.51.100.1", "b1")]),
            ],
            move |proxy| {
                let started = observed.clone();
                async move {
                    started
                        .lock()
                        .unwrap()
                        .push(proxy.source.rsplit(':').next().unwrap().to_string());
                    Ok(ok_sample())
                }
            },
            1000,
            1,
            |_| {},
        )
        .await;
        assert_eq!(*started.lock().unwrap(), vec!["a1", "b1", "a2"]);
    }

    #[tokio::test]
    async fn schedule_drops_failures_and_reports_ok_count() {
        let a = "192.0.2.0/24".to_string();
        let buckets = vec![bucket(
            "192.0.2.1",
            vec![
                proxy("192.0.2.1", "ok"),
                proxy("192.0.2.2", "fail"),
                proxy("192.0.2.3", "ok"),
            ],
        )];
        let patches = Arc::new(StdMutex::new(Vec::new()));
        let observed = patches.clone();
        let result = schedule(
            buckets,
            |proxy| async move {
                if proxy.source.ends_with(":fail") {
                    Err(())
                } else {
                    Ok(ok_sample())
                }
            },
            move |progress| {
                observed.lock().unwrap().push(progress);
            },
        )
        .await;
        let metrics = result.get(&a).unwrap();
        assert_eq!(metrics.tested, 3);
        assert_eq!(metrics.ok, 2);
        assert_eq!(metrics.connect_p50, Some(10.0));
        let events = patches.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events.last().unwrap().done, 3);
        assert_eq!(events.last().unwrap().total, 3);
        assert_eq!(events.last().unwrap().metrics.ok, 2);
    }

    #[tokio::test]
    async fn schedule_never_exceeds_in_flight_cap() {
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let buckets = vec![bucket(
            "192.0.2.1",
            (1..=8)
                .map(|n| proxy("192.0.2.1", &n.to_string()))
                .collect(),
        )];
        let current_probe = current.clone();
        let peak_probe = peak.clone();
        schedule_with(
            buckets,
            move |_proxy| {
                let current = current_probe.clone();
                let peak = peak_probe.clone();
                async move {
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(ok_sample())
                }
            },
            1000,
            3,
            |_| {},
        )
        .await;
        assert!(peak.load(Ordering::SeqCst) <= 3);
        assert_eq!(peak.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn eta_none_at_start_and_end() {
        assert_eq!(eta(Duration::from_secs(1), 0, 10), None);
        assert_eq!(eta(Duration::from_secs(1), 10, 10), None);
        assert_eq!(eta(Duration::from_secs(10), 5, 10), Some(10));
    }
}

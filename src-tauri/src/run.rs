use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, Semaphore};

use crate::cancel::Cancel;
use crate::measurement::Tally;
use crate::probe::Sample;
use crate::proxy::ProxyLine;
use crate::settings::Settings;
use crate::subnet::Subnet;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub in_flight: usize,
    pub starts_per_second: usize,
}

impl From<&Settings> for Limits {
    fn from(settings: &Settings) -> Self {
        Self {
            in_flight: settings.in_flight,
            starts_per_second: settings.starts_per_second,
        }
    }
}

pub struct Workload {
    pub subnet: Subnet,
    pub proxies: Vec<ProxyLine>,
}

pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub eta_seconds: Option<u64>,
    pub subnet: Subnet,
    pub tally: Tally,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Completed,
    Stopped,
}

pub async fn schedule<Job, Pending, OnProgress>(
    workloads: Vec<Workload>,
    limits: Limits,
    cancel: Cancel,
    job: Job,
    mut on_progress: OnProgress,
) -> Ending
where
    Job: Fn(ProxyLine) -> Pending + Send + Sync + 'static,
    Pending: Future<Output = Option<Sample>> + Send,
    OnProgress: FnMut(Progress) + Send,
{
    let work = round_robin(workloads);
    let total = work.len();
    let mut tallies: HashMap<Subnet, Tally> = work
        .iter()
        .map(|(subnet, _)| (*subnet, Tally::new()))
        .collect();
    if total == 0 {
        return Ending::Completed;
    }
    let started = Instant::now();
    let rate = Arc::new(RateLimit::new(limits.starts_per_second));
    let in_flight = Arc::new(Semaphore::new(limits.in_flight));
    let job = Arc::new(job);
    let (sender, mut results) = tokio::sync::mpsc::channel(total);
    let dispatcher = tokio::spawn({
        let cancel = cancel.clone();
        async move {
            for (subnet, proxy) in work {
                let permit = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    permit = in_flight.clone().acquire_owned() => permit.expect("semaphore stays open"),
                };
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    _ = rate.acquire() => {}
                }
                let job = job.clone();
                let sender = sender.clone();
                let cancel = cancel.clone();
                tokio::spawn(async move {
                    let sample = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return,
                        sample = job(proxy) => sample,
                    };
                    drop(permit);
                    let _ = sender.send((subnet, sample)).await;
                });
            }
        }
    });
    let mut done = 0;
    while let Some((subnet, sample)) = results.recv().await {
        done += 1;
        let tally = tallies.get_mut(&subnet).expect("scheduled subnet");
        tally.record(sample);
        on_progress(Progress {
            done,
            total,
            eta_seconds: eta(started.elapsed(), done, total),
            subnet,
            tally: tally.clone(),
        });
    }
    let _ = dispatcher.await;
    if done < total {
        Ending::Stopped
    } else {
        Ending::Completed
    }
}

fn round_robin(workloads: Vec<Workload>) -> Vec<(Subnet, ProxyLine)> {
    let mut queues: Vec<(Subnet, VecDeque<ProxyLine>)> = workloads
        .into_iter()
        .map(|workload| (workload.subnet, workload.proxies.into()))
        .collect();
    let mut work = Vec::new();
    loop {
        let before = work.len();
        for (subnet, queue) in &mut queues {
            if let Some(proxy) = queue.pop_front() {
                work.push((*subnet, proxy));
            }
        }
        if work.len() == before {
            return work;
        }
    }
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

    fn subnet(host: &str) -> Subnet {
        Subnet::from_host(host.parse().unwrap())
    }

    fn workload(host: &str, proxies: Vec<ProxyLine>) -> Workload {
        Workload {
            subnet: subnet(host),
            proxies,
        }
    }

    fn label(proxy: &ProxyLine) -> &str {
        proxy.source.rsplit(':').next().unwrap()
    }

    fn ok_sample() -> Sample {
        Sample {
            connect: Duration::from_millis(10),
            ttfb: Duration::from_millis(20),
        }
    }

    type Tested = Arc<StdMutex<HashMap<Subnet, usize>>>;

    fn record_tested(tested: &Tested) -> impl FnMut(Progress) + Send {
        let writer = tested.clone();
        move |progress: Progress| {
            writer
                .lock()
                .unwrap()
                .insert(progress.subnet, progress.tally.probe(0).tested);
        }
    }

    fn limits(in_flight: usize) -> Limits {
        Limits {
            in_flight,
            starts_per_second: 1000,
        }
    }

    #[test]
    fn round_robin_interleaves_subnets() {
        let work = round_robin(vec![
            workload(
                "192.0.2.1",
                vec![proxy("192.0.2.1", "a1"), proxy("192.0.2.2", "a2")],
            ),
            workload("198.51.100.1", vec![proxy("198.51.100.1", "b1")]),
        ]);
        let labels: Vec<_> = work.iter().map(|(_, proxy)| label(proxy)).collect();
        assert_eq!(labels, vec!["a1", "b1", "a2"]);
    }

    #[tokio::test]
    async fn schedule_starts_in_round_robin_order() {
        let started = Arc::new(StdMutex::new(Vec::new()));
        let observed = started.clone();
        schedule(
            vec![
                workload(
                    "192.0.2.1",
                    vec![proxy("192.0.2.1", "a1"), proxy("192.0.2.2", "a2")],
                ),
                workload("198.51.100.1", vec![proxy("198.51.100.1", "b1")]),
            ],
            limits(1),
            Cancel::new(),
            move |proxy| {
                let started = observed.clone();
                async move {
                    started.lock().unwrap().push(label(&proxy).to_string());
                    Some(ok_sample())
                }
            },
            |_| {},
        )
        .await;
        assert_eq!(*started.lock().unwrap(), vec!["a1", "b1", "a2"]);
    }

    #[tokio::test]
    async fn schedule_drops_failures_and_reports_progress_per_sample() {
        let events = Arc::new(StdMutex::new(Vec::new()));
        let observed = events.clone();
        let ending = schedule(
            vec![workload(
                "192.0.2.1",
                vec![
                    proxy("192.0.2.1", "ok"),
                    proxy("192.0.2.2", "fail"),
                    proxy("192.0.2.3", "ok"),
                ],
            )],
            limits(8),
            Cancel::new(),
            |proxy| async move {
                if label(&proxy) == "fail" {
                    None
                } else {
                    Some(ok_sample())
                }
            },
            move |progress| observed.lock().unwrap().push(progress),
        )
        .await;
        assert_eq!(ending, Ending::Completed);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
        let last = events.last().unwrap();
        assert_eq!((last.done, last.total), (3, 3));
        let probe = last.tally.probe(0);
        assert_eq!((probe.ok, probe.tested), (2, 3));
        assert_eq!(probe.connect_p50, Some(10.0));
    }

    #[tokio::test]
    async fn schedule_never_exceeds_in_flight_cap() {
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let proxies = (1..=8)
            .map(|n| proxy("192.0.2.1", &n.to_string()))
            .collect();
        let current_job = current.clone();
        let peak_job = peak.clone();
        schedule(
            vec![workload("192.0.2.1", proxies)],
            limits(3),
            Cancel::new(),
            move |_proxy| {
                let current = current_job.clone();
                let peak = peak_job.clone();
                async move {
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Some(ok_sample())
                }
            },
            |_| {},
        )
        .await;
        assert_eq!(peak.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn cancel_returns_promptly_with_completed_samples_only() {
        let cancel = Cancel::new();
        let trigger = cancel.clone();
        let started = Instant::now();
        let tested: Tested = Arc::default();
        let mut record = record_tested(&tested);
        let ending = schedule(
            vec![
                workload(
                    "192.0.2.1",
                    vec![proxy("192.0.2.1", "fast"), proxy("192.0.2.2", "slow")],
                ),
                workload("198.51.100.1", vec![proxy("198.51.100.1", "slow")]),
            ],
            limits(8),
            cancel,
            |proxy| async move {
                if label(&proxy) == "slow" {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
                Some(ok_sample())
            },
            move |progress| {
                record(progress);
                trigger.cancel();
            },
        )
        .await;
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(ending, Ending::Stopped);
        let tested = tested.lock().unwrap();
        assert_eq!(tested.get(&subnet("192.0.2.1")), Some(&1));
        assert_eq!(tested.get(&subnet("198.51.100.1")), None);
    }

    #[tokio::test]
    async fn cancel_before_start_runs_nothing() {
        let cancel = Cancel::new();
        cancel.cancel();
        let ran = Arc::new(AtomicUsize::new(0));
        let counted = ran.clone();
        let ending = schedule(
            vec![workload("192.0.2.1", vec![proxy("192.0.2.1", "a")])],
            limits(8),
            cancel,
            move |_| {
                counted.fetch_add(1, Ordering::SeqCst);
                async { Some(ok_sample()) }
            },
            |_| {},
        )
        .await;
        assert_eq!(ending, Ending::Stopped);
        assert_eq!(ran.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn empty_workload_completes_immediately() {
        let ending = schedule(
            Vec::new(),
            limits(8),
            Cancel::new(),
            |_| async { Some(ok_sample()) },
            |_| {},
        )
        .await;
        assert_eq!(ending, Ending::Completed);
    }

    #[test]
    fn eta_none_at_start_and_end() {
        assert_eq!(eta(Duration::from_secs(1), 0, 10), None);
        assert_eq!(eta(Duration::from_secs(1), 10, 10), None);
        assert_eq!(eta(Duration::from_secs(10), 5, 10), Some(10));
    }
}

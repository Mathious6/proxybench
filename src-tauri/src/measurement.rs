use std::time::Duration;

use serde::Serialize;

use crate::health::Health;
use crate::probe::Sample;
use crate::stats::{self, milliseconds};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub at: u64,
    pub ok: usize,
    pub tested: usize,
    pub connect_p50: Option<f64>,
    pub connect_p95: Option<f64>,
    pub ttfb_p50: Option<f64>,
    pub ttfb_p95: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct Tally {
    tested: usize,
    ok: usize,
    connect: Vec<Duration>,
    ttfb: Vec<Duration>,
}

impl Tally {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, sample: Option<Sample>) {
        self.tested += 1;
        if let Some(sample) = sample {
            self.ok += 1;
            self.connect.push(sample.connect);
            self.ttfb.push(sample.ttfb);
        }
    }

    pub fn health(&self, at: u64) -> Health {
        Health::new(self.ok, self.tested, at)
    }

    pub fn probe(&self, at: u64) -> Probe {
        let connect = stats::percentiles(&self.connect);
        let ttfb = stats::percentiles(&self.ttfb);
        Probe {
            at,
            ok: self.ok,
            tested: self.tested,
            connect_p50: connect.map(|value| milliseconds(value.p50)),
            connect_p95: connect.map(|value| milliseconds(value.p95)),
            ttfb_p50: ttfb.map(|value| milliseconds(value.p50)),
            ttfb_p95: ttfb.map(|value| milliseconds(value.p95)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Status;

    fn sample(connect: u64, ttfb: u64) -> Option<Sample> {
        Some(Sample {
            connect: Duration::from_millis(connect),
            ttfb: Duration::from_millis(ttfb),
        })
    }

    #[test]
    fn failures_count_as_tested_but_not_in_percentiles() {
        let mut tally = Tally::new();
        tally.record(sample(10, 20));
        tally.record(None);
        tally.record(sample(30, 60));
        let probe = tally.probe(5);
        assert_eq!((probe.ok, probe.tested, probe.at), (2, 3, 5));
        assert_eq!(probe.connect_p50, Some(10.0));
        assert_eq!(probe.connect_p95, Some(30.0));
        assert_eq!(probe.ttfb_p95, Some(60.0));
    }

    #[test]
    fn all_failed_has_no_percentiles_and_dead_health() {
        let mut tally = Tally::new();
        tally.record(None);
        let probe = tally.probe(1);
        assert_eq!(probe.connect_p50, None);
        assert_eq!(probe.ttfb_p50, None);
        assert_eq!(tally.health(1).status, Status::Dead);
    }

    #[test]
    fn health_reflects_success_ratio() {
        let mut tally = Tally::new();
        tally.record(sample(1, 2));
        tally.record(None);
        assert_eq!(tally.health(9).status, Status::Partial);
        assert_eq!(tally.probe(0).tested, 2);
    }
}

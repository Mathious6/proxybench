use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Percentiles {
    pub p50: Duration,
    pub p95: Duration,
}

pub fn percentiles(samples: &[Duration]) -> Option<Percentiles> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort();
    Some(Percentiles {
        p50: nearest_rank(&sorted, 50),
        p95: nearest_rank(&sorted, 95),
    })
}

pub fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn nearest_rank(sorted: &[Duration], percent: u8) -> Duration {
    let n = sorted.len();
    let rank = (percent as usize * n).div_ceil(100);
    sorted[rank.saturating_sub(1).min(n - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn percentiles_none_when_empty() {
        assert_eq!(percentiles(&[]), None);
    }

    #[test]
    fn percentiles_one_sample_is_both_p50_and_p95() {
        let got = percentiles(&[ms(12)]).unwrap();
        assert_eq!(got.p50, ms(12));
        assert_eq!(got.p95, ms(12));
    }

    #[test]
    fn percentiles_even_count_uses_nearest_rank() {
        let samples = [ms(1), ms(2), ms(3), ms(4)];
        let got = percentiles(&samples).unwrap();
        assert_eq!(got.p50, ms(2));
        assert_eq!(got.p95, ms(4));
    }

    #[test]
    fn percentiles_known_p95_set() {
        let samples: Vec<_> = (1..=20).map(ms).collect();
        let got = percentiles(&samples).unwrap();
        assert_eq!(got.p50, ms(10));
        assert_eq!(got.p95, ms(19));
    }

    #[test]
    fn milliseconds_converts_duration() {
        assert_eq!(milliseconds(ms(1500)), 1500.0);
    }
}

use std::sync::{Mutex, MutexGuard};

use tokio::sync::Notify;

use crate::cancel::Cancel;
use crate::wire::RunKind;

const BUSY: &str = "A run is already in progress.";

struct Active {
    kind: RunKind,
    cancel: Cancel,
}

enum Claim {
    Granted(Cancel),
    Busy,
    Preempting,
}

#[derive(Default)]
pub struct RunSlot {
    active: Mutex<Option<Active>>,
    freed: Notify,
}

pub struct RunGuard<'a> {
    slot: &'a RunSlot,
    pub cancel: Cancel,
}

impl RunSlot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_check(&self) -> Result<RunGuard<'_>, String> {
        match self.claim(RunKind::Check) {
            Claim::Granted(cancel) => Ok(RunGuard { slot: self, cancel }),
            Claim::Busy | Claim::Preempting => Err(BUSY.into()),
        }
    }

    pub async fn begin_probe(&self) -> Result<RunGuard<'_>, String> {
        loop {
            let freed = self.freed.notified();
            match self.claim(RunKind::Probe) {
                Claim::Granted(cancel) => return Ok(RunGuard { slot: self, cancel }),
                Claim::Busy => return Err(BUSY.into()),
                Claim::Preempting => freed.await,
            }
        }
    }

    pub fn stop(&self) {
        if let Some(active) = self.lock().as_ref() {
            active.cancel.cancel();
        }
    }

    fn claim(&self, kind: RunKind) -> Claim {
        let mut active = self.lock();
        match active.as_ref() {
            None => {
                let cancel = Cancel::new();
                *active = Some(Active {
                    kind,
                    cancel: cancel.clone(),
                });
                Claim::Granted(cancel)
            }
            Some(running) if running.kind == RunKind::Check && kind == RunKind::Probe => {
                running.cancel.cancel();
                Claim::Preempting
            }
            Some(_) => Claim::Busy,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Active>> {
        self.active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        *self.slot.lock() = None;
        self.slot.freed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn only_one_run_at_a_time() {
        let slot = RunSlot::new();
        let guard = slot.begin_check().unwrap();
        assert!(slot.begin_check().is_err());
        drop(guard);
        assert!(slot.begin_check().is_ok());
    }

    #[tokio::test]
    async fn probe_is_rejected_while_another_probe_runs() {
        let slot = RunSlot::new();
        let _probe = slot.begin_probe().await.unwrap();
        assert!(slot.begin_probe().await.is_err());
        assert!(slot.begin_check().is_err());
    }

    #[tokio::test]
    async fn probe_preempts_a_running_check_and_waits_for_it_to_finish() {
        let slot = std::sync::Arc::new(RunSlot::new());
        let check_slot = slot.clone();
        let check = tokio::spawn(async move {
            let guard = check_slot.begin_check().unwrap();
            guard.cancel.cancelled().await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(guard);
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let probe = tokio::time::timeout(Duration::from_secs(2), slot.begin_probe())
            .await
            .expect("probe got the slot")
            .unwrap();
        check.await.unwrap();
        assert!(slot.begin_check().is_err());
        drop(probe);
    }

    #[tokio::test]
    async fn stop_cancels_the_active_run_and_is_harmless_when_idle() {
        let slot = RunSlot::new();
        slot.stop();
        let guard = slot.begin_check().unwrap();
        slot.stop();
        tokio::time::timeout(Duration::from_secs(1), guard.cancel.cancelled())
            .await
            .unwrap();
    }
}

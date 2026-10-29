use std::sync::Arc;

use tokio::sync::watch;

#[derive(Clone)]
pub struct Cancel(Arc<watch::Sender<bool>>);

impl Cancel {
    pub fn new() -> Self {
        Self(Arc::new(watch::channel(false).0))
    }

    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        let _ = receiver.wait_for(|cancelled| *cancelled).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn cancelled_resolves_after_cancel_even_for_late_waiters() {
        let cancel = Cancel::new();
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(1), cancel.cancelled())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cancelled_stays_pending_until_cancel() {
        let cancel = Cancel::new();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), cancel.cancelled())
                .await
                .is_err()
        );
    }
}

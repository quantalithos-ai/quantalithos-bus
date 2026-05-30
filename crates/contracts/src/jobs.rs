//! Job DTOs and summaries for bus operations workers.

use serde::{Deserialize, Serialize};

use crate::metadata::{BackendId, DeliveryScanCursor, JobRunId};

/// Scans schedulable deliveries and progresses them through the default backend path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunDeliveryProgressionJob {
    /// The unique job run identifier.
    pub job_run_id: JobRunId,
    /// The schedulable-delivery scan cursor.
    pub cursor: DeliveryScanCursor,
    /// The maximum number of deliveries to scan in the current batch.
    pub batch_size: u32,
    /// The logical backend identifier selected for the batch.
    pub backend_id: BackendId,
}

/// The summary returned after a delivery-progression job run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryProgressionResult {
    /// The unique job run identifier.
    pub job_run_id: JobRunId,
    /// The number of deliveries scanned in the batch.
    pub scanned: u32,
    /// The number of deliveries that reached `Delivered`.
    pub dispatched: u32,
    /// The number of deliveries skipped because the current truth no longer matched.
    pub skipped: u32,
    /// The next cursor to use for a following batch.
    pub next_cursor: DeliveryScanCursor,
}

impl DeliveryProgressionResult {
    /// Starts a new delivery-progression summary.
    pub fn start(job_run_id: JobRunId, next_cursor: DeliveryScanCursor) -> Self {
        Self {
            job_run_id,
            scanned: 0,
            dispatched: 0,
            skipped: 0,
            next_cursor,
        }
    }

    /// Records one delivered item.
    pub fn record_dispatched(&mut self) {
        self.scanned += 1;
        self.dispatched += 1;
    }

    /// Records one skipped item.
    pub fn record_skipped(&mut self) {
        self.scanned += 1;
        self.skipped += 1;
    }

    /// Records one failed item that committed `DeliveryStatus::Failed`.
    pub fn record_failed(&mut self) {
        self.scanned += 1;
    }

    /// Updates the next scan cursor.
    pub fn set_next_cursor(&mut self, next_cursor: DeliveryScanCursor) {
        self.next_cursor = next_cursor;
    }

    /// Returns the number of failed items in the batch.
    pub fn failed(&self) -> u32 {
        self.scanned - self.dispatched - self.skipped
    }
}

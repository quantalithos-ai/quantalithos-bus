//! Job DTOs and summaries for bus operations workers.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    BackendId, DeliveryScanCursor, JobRunId, OutboxCursor, RetryScanCursor, Timestamp,
};

/// Scans committed outbox facts and relays them into bus publication acceptance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunOutboxRelayJob {
    /// The unique job run identifier.
    pub job_run_id: JobRunId,
    /// The committed-outbox scan cursor.
    pub cursor: OutboxCursor,
    /// The maximum number of facts to scan in the current batch.
    pub batch_size: u32,
    /// Whether the job should avoid mutating downstream state.
    pub dry_run: bool,
}

/// The summary returned after an outbox-relay job run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboxRelayJobResult {
    /// The unique job run identifier.
    pub job_run_id: JobRunId,
    /// The number of facts scanned in the batch.
    pub scanned: u32,
    /// The number of facts that were accepted or deduplicated successfully.
    pub accepted: u32,
    /// The number of facts that were committed as rejected.
    pub rejected: u32,
    /// The next cursor to use for a following batch.
    pub next_cursor: OutboxCursor,
}

impl OutboxRelayJobResult {
    /// Starts a new outbox-relay summary.
    pub fn start(job_run_id: JobRunId, next_cursor: OutboxCursor) -> Self {
        Self {
            job_run_id,
            scanned: 0,
            accepted: 0,
            rejected: 0,
            next_cursor,
        }
    }

    /// Records one accepted or deduplicated item.
    pub fn record_accepted(&mut self) {
        self.scanned += 1;
        self.accepted += 1;
    }

    /// Records one rejected item.
    pub fn record_rejected(&mut self) {
        self.scanned += 1;
        self.rejected += 1;
    }

    /// Records one failed item.
    pub fn record_failed(&mut self) {
        self.scanned += 1;
    }

    /// Updates the next scan cursor.
    pub fn set_next_cursor(&mut self, next_cursor: OutboxCursor) {
        self.next_cursor = next_cursor;
    }

    /// Returns the number of failed items in the batch.
    pub fn failed(&self) -> u32 {
        self.scanned - self.accepted - self.rejected
    }
}

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

/// Scans due retry plans and executes one retry attempt per eligible plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRetryCycleJob {
    /// The unique job run identifier.
    pub job_run_id: JobRunId,
    /// The due-retry scan cursor.
    pub cursor: RetryScanCursor,
    /// The maximum number of retry plans to scan in the current batch.
    pub batch_size: u32,
    /// The stable execution timestamp used by the current batch.
    pub now: Timestamp,
}

/// The summary returned after a retry-cycle job run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryCycleResult {
    /// The unique job run identifier.
    pub job_run_id: JobRunId,
    /// The number of retry plans scanned in the batch.
    pub scanned: u32,
    /// The number of retry plans that produced a committed retry attempt.
    pub retried: u32,
    /// The number of retry plans that were committed as exhausted.
    pub exhausted: u32,
    /// The next cursor to use for a following batch.
    pub next_cursor: RetryScanCursor,
}

impl RetryCycleResult {
    /// Starts a new retry-cycle summary.
    pub fn start(job_run_id: JobRunId, next_cursor: RetryScanCursor) -> Self {
        Self {
            job_run_id,
            scanned: 0,
            retried: 0,
            exhausted: 0,
            next_cursor,
        }
    }

    /// Records one committed retry attempt.
    pub fn record_retried(&mut self) {
        self.scanned += 1;
        self.retried += 1;
    }

    /// Records one committed exhausted retry plan.
    pub fn record_exhausted(&mut self) {
        self.scanned += 1;
        self.exhausted += 1;
    }

    /// Records one failed retry-plan item.
    pub fn record_failed(&mut self) {
        self.scanned += 1;
    }

    /// Updates the next scan cursor.
    pub fn set_next_cursor(&mut self, next_cursor: RetryScanCursor) {
        self.next_cursor = next_cursor;
    }

    /// Returns the number of failed items in the batch.
    pub fn failed(&self) -> u32 {
        self.scanned - self.retried - self.exhausted
    }
}

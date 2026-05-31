//! Unit-of-work transaction port.

use bus_contracts::metadata::ActorContext;

use crate::errors::UnitOfWorkError;

/// A write-transaction purpose label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitOfWorkPurpose {
    /// The publication acceptance write path.
    AcceptPublication,
    /// The committed outbox fact consumer write path.
    ConsumeCommittedOutboxFact,
    /// The delivery progression write path.
    RunDeliveryProgression,
    /// The delivery feedback write path.
    RecordDeliveryFeedback,
    /// The backend delivery signal consumer write path.
    ConsumeBackendDeliverySignal,
    /// The timeout signal consumer write path.
    ConsumeTimeoutSignal,
    /// The retry-request write path.
    RequestRetry,
    /// The retry-cycle job item write path.
    RunRetryCycle,
    /// The dead-letter write path.
    MoveDeliveryToDeadLetter,
    /// The replay-preparation write path.
    PrepareReplay,
}

/// A rollback reason label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RollbackReason {
    /// The flow failed with the provided stable code.
    ApplicationError(&'static str),
}

/// An opaque transaction handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnitOfWorkHandle {
    /// The transaction identifier.
    pub transaction_id: u64,
    /// The transaction purpose.
    pub purpose: UnitOfWorkPurpose,
}

/// A commit receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    /// The committed transaction identifier.
    pub transaction_id: u64,
}

/// A write transaction boundary.
pub trait UnitOfWork: Send + Sync {
    /// Begins a new write transaction.
    async fn begin(
        &self,
        purpose: UnitOfWorkPurpose,
        actor: ActorContext,
    ) -> Result<UnitOfWorkHandle, UnitOfWorkError>;

    /// Commits a write transaction.
    async fn commit(&self, handle: UnitOfWorkHandle) -> Result<CommitReceipt, UnitOfWorkError>;

    /// Rolls back a write transaction.
    async fn rollback(
        &self,
        handle: UnitOfWorkHandle,
        reason: RollbackReason,
    ) -> Result<(), UnitOfWorkError>;
}

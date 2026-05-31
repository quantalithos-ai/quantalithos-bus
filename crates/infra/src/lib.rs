//! In-memory infrastructure adapters for the L0-bus workspace.

mod id;
mod observability;
mod outbox;
mod repositories;
mod source;
mod store;
mod time;
mod uow;

pub use id::DeterministicIdGenerator;
pub use observability::{SharedTapOutputSink, TapOutputRecord};
pub use outbox::{InMemoryOutboxPublisherAdapter, PublishedEventRecord, SharedPublishedEventSink};
pub use repositories::{
    InMemoryAuditTrailRepository, InMemoryDeliveryRepository, InMemoryFeedbackRepository,
    InMemoryIdempotencyRepository, InMemoryPublicationRepository, InMemoryReadProjectionRepository,
    InMemoryRecoveryRepository, InMemoryTransportBackendAdapter,
};
pub use source::{InMemoryOutboxFactSourceAdapter, OutboxSourceFixtureError, SharedOutboxSource};
pub use store::SharedMemoryStore;
pub use time::FixedClockAdapter;
pub use uow::InMemoryUnitOfWork;

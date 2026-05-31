//! Application service port definitions.

mod audit;
mod clock;
mod delivery;
mod feedback;
mod id_generator;
mod idempotency;
mod outbox_source;
mod projection;
mod publication;
mod recovery;
mod transport;
mod unit_of_work;

pub use audit::AuditTrailRepository;
pub use clock::ClockPort;
pub use delivery::DeliveryRepository;
pub use feedback::FeedbackRepository;
pub use id_generator::{BusRecordKind, IdGeneratorPort};
pub use idempotency::IdempotencyRepository;
pub use outbox_source::OutboxFactSourcePort;
pub use projection::ReadProjectionRepository;
pub use publication::PublicationRepository;
pub use recovery::RecoveryRepository;
pub use transport::{BackendCapabilityReport, BackendDispatchContext, TransportBackendPort};
pub use unit_of_work::{
    CommitReceipt, RollbackReason, UnitOfWork, UnitOfWorkHandle, UnitOfWorkPurpose,
};

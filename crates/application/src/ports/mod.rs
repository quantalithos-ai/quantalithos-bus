//! Application service port definitions.

mod audit;
mod clock;
mod delivery;
mod id_generator;
mod idempotency;
mod publication;
mod transport;
mod unit_of_work;

pub use audit::AuditTrailRepository;
pub use clock::ClockPort;
pub use delivery::DeliveryRepository;
pub use id_generator::{BusRecordKind, IdGeneratorPort};
pub use idempotency::IdempotencyRepository;
pub use publication::PublicationRepository;
pub use transport::{BackendCapabilityReport, BackendDispatchContext, TransportBackendPort};
pub use unit_of_work::{
    CommitReceipt, RollbackReason, UnitOfWork, UnitOfWorkHandle, UnitOfWorkPurpose,
};

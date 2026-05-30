//! In-memory infrastructure adapters for the L0-bus workspace.

mod id;
mod repositories;
mod store;
mod time;
mod uow;

pub use id::DeterministicIdGenerator;
pub use repositories::{
    InMemoryAuditTrailRepository, InMemoryIdempotencyRepository, InMemoryPublicationRepository,
};
pub use store::SharedMemoryStore;
pub use time::FixedClockAdapter;
pub use uow::InMemoryUnitOfWork;

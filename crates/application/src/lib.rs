//! Application services, ports, and error mapping for L0-bus.

#![allow(async_fn_in_trait)]

pub mod errors;
pub mod ports;
pub mod services;

pub use errors::{
    ApplicationError, BoundaryViolationError, ConflictError, DependencyError, ErrorDetailsRef,
    IdGenerationError, InternalError, NotFoundError, ProtocolErrorCategory, RepositoryError,
    UnitOfWorkError, ValidationError,
};
pub use ports::{
    AuditTrailRepository, BusRecordKind, ClockPort, CommitReceipt, IdGeneratorPort,
    IdempotencyRepository, PublicationRepository, RollbackReason, UnitOfWork, UnitOfWorkHandle,
    UnitOfWorkPurpose,
};
pub use services::{
    PublicationAcceptanceService, PublicationAcceptanceServiceDeps, PublicationAcceptanceUseCase,
};

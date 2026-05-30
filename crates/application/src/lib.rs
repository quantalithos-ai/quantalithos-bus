//! Application services, ports, and error mapping for L0-bus.

#![allow(async_fn_in_trait)]

pub mod errors;
pub mod ports;
pub mod services;

pub use errors::{
    ApplicationError, BoundaryViolationError, ConflictError, DependencyError, ErrorDetailsRef,
    IdGenerationError, InternalError, NotFoundError, ProtocolErrorCategory, RepositoryError,
    SourcePortError, TransportPortError, UnitOfWorkError, ValidationError,
};
pub use ports::{
    AuditTrailRepository, BackendCapabilityReport, BackendDispatchContext, BusRecordKind,
    ClockPort, CommitReceipt, DeliveryRepository, IdGeneratorPort, IdempotencyRepository,
    OutboxFactSourcePort, PublicationRepository, RollbackReason, TransportBackendPort, UnitOfWork,
    UnitOfWorkHandle, UnitOfWorkPurpose,
};
pub use services::{
    DeliveryProgressionItemResult, DeliveryProgressionService, DeliveryProgressionServiceDeps,
    DeliveryProgressionUseCase, PublicationAcceptanceService, PublicationAcceptanceServiceDeps,
    PublicationAcceptanceUseCase,
};

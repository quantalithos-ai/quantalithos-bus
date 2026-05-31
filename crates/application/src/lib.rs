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
    ClockPort, CommitReceipt, DeliveryRepository, FeedbackRepository, IdGeneratorPort,
    IdempotencyRepository, OutboxFactSourcePort, PublicationRepository, RecoveryRepository,
    RollbackReason, TransportBackendPort, UnitOfWork, UnitOfWorkHandle, UnitOfWorkPurpose,
};
pub use services::{
    BackendSignalUseCase, DeliveryFeedbackUseCase, DeliveryProgressionItemResult,
    DeliveryProgressionService, DeliveryProgressionServiceDeps, DeliveryProgressionUseCase,
    FeedbackRecordingService, FeedbackRecordingServiceDeps, MoveToDeadLetterUseCase,
    OutboxPublicationAcceptanceUseCase, OutboxRelayService, OutboxRelayServiceDeps,
    OutboxRelayUseCase, PublicationAcceptanceService, PublicationAcceptanceServiceDeps,
    PublicationAcceptanceUseCase, RecoveryOrchestrationService, RecoveryOrchestrationServiceDeps,
    ReplayPreparationService, ReplayPreparationServiceDeps, ReplayPreparationUseCase,
    RequestRetryUseCase, RetryCycleUseCase, TimeoutSignalUseCase,
};

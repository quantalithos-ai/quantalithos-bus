//! Application services, ports, and error mapping for L0-bus.

#![allow(async_fn_in_trait)]

pub mod errors;
pub mod ports;
pub mod services;

pub use errors::{
    ApplicationError, BoundaryViolationError, ConflictError, DependencyError, ErrorDetailsRef,
    IdGenerationError, InternalError, NotFoundError, ProtocolErrorCategory, PublisherPortError,
    RepositoryError, SourcePortError, TransportPortError, UnitOfWorkError, ValidationError,
};
pub use ports::{
    AuditTrailRepository, BackendCapabilityReport, BackendDispatchContext, BusRecordKind,
    ClockPort, CommitReceipt, DeliveryRepository, FeedbackRepository, IdGeneratorPort,
    IdempotencyRepository, OutboxFactSourcePort, OutboxPublisherPort, PublicationRepository,
    PublishBatchReceipt, PublishEvidenceRecord, PublishEvidenceStatus, PublishReceipt,
    ReadProjectionRepository, RecoveryRepository, RollbackReason, TransportBackendPort, UnitOfWork,
    UnitOfWorkHandle, UnitOfWorkPurpose,
};
pub use services::{
    BackendSignalUseCase, DeliveryFeedbackUseCase, DeliveryProgressionItemResult,
    DeliveryProgressionService, DeliveryProgressionServiceDeps, DeliveryProgressionUseCase,
    FeedbackRecordingService, FeedbackRecordingServiceDeps, MoveToDeadLetterUseCase,
    OutboxPublicationAcceptanceUseCase, OutboxPublisherService, OutboxPublisherServiceDeps,
    OutboxPublisherUseCase, OutboxRelayService, OutboxRelayServiceDeps, OutboxRelayUseCase,
    PublicationAcceptanceService, PublicationAcceptanceServiceDeps, PublicationAcceptanceUseCase,
    ReadOutputService, ReadOutputServiceDeps, ReadOutputUseCase, RecoveryOrchestrationService,
    RecoveryOrchestrationServiceDeps, ReplayPreparationService, ReplayPreparationServiceDeps,
    ReplayPreparationUseCase, RequestRetryUseCase, RetryCycleUseCase, TimeoutSignalUseCase,
};

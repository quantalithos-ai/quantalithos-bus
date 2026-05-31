//! Outbound event publisher service.

use bus_contracts::events::{
    BusOutboundEvent, BusOutboundEventBatch, OutboundEventValidationError,
};
use bus_contracts::metadata::TraceContextRef;
use bus_domain::publication::PayloadBoundaryGuard;

use crate::errors::ApplicationError;
use crate::ports::{OutboxPublisherPort, PublishBatchReceipt, PublishReceipt};

/// Outbound publisher use-case contract.
pub trait OutboxPublisherUseCase: Send + Sync {
    /// Publishes one committed outbound event.
    async fn publish_committed(
        &self,
        event: BusOutboundEvent,
        trace: TraceContextRef,
    ) -> Result<PublishReceipt, ApplicationError>;

    /// Publishes one batch of committed outbound events.
    async fn publish_batch_committed(
        &self,
        batch: BusOutboundEventBatch,
        trace: TraceContextRef,
    ) -> Result<PublishBatchReceipt, ApplicationError>;
}

/// Service dependencies for outbound publisher flow.
pub struct OutboxPublisherServiceDeps<P> {
    /// The configured outbound publisher adapter.
    pub publisher: P,
}

/// The committed outbound publisher service.
pub struct OutboxPublisherService<P> {
    deps: OutboxPublisherServiceDeps<P>,
    payload_guard: PayloadBoundaryGuard,
}

impl<P> OutboxPublisherService<P> {
    /// Creates a new outbound publisher service.
    pub fn new(deps: OutboxPublisherServiceDeps<P>) -> Self {
        Self {
            deps,
            payload_guard: PayloadBoundaryGuard::default_for_bus(),
        }
    }

    fn validate_event(&self, event: &BusOutboundEvent) -> Result<(), ApplicationError> {
        if let Some(payload_ref) = event.payload_ref() {
            if !self.payload_guard.allows_reference(payload_ref.clone()) {
                return Err(ApplicationError::boundary_violation(
                    "boundary.outbound_event_payload_ref_rejected",
                    "outbound event payload must remain reference-only",
                    None,
                ));
            }
        }

        event.validate_schema().map_err(|error| match error {
            OutboundEventValidationError::InvalidSchemaVersion => {
                ApplicationError::boundary_violation(
                    "boundary.outbound_event_schema_rejected",
                    "outbound event schema version is invalid",
                    None,
                )
            }
            OutboundEventValidationError::MissingField(field) => ApplicationError::validation(
                "validation.outbound_event",
                format!("outbound event field is missing: {field}"),
            ),
            OutboundEventValidationError::ForbiddenPayloadReference => {
                ApplicationError::boundary_violation(
                    "boundary.outbound_event_payload_ref_rejected",
                    "outbound event payload must remain reference-only",
                    None,
                )
            }
        })
    }
}

impl<P> OutboxPublisherUseCase for OutboxPublisherService<P>
where
    P: OutboxPublisherPort,
{
    async fn publish_committed(
        &self,
        event: BusOutboundEvent,
        trace: TraceContextRef,
    ) -> Result<PublishReceipt, ApplicationError> {
        self.validate_event(&event)?;
        self.deps
            .publisher
            .publish(event, trace)
            .await
            .map_err(ApplicationError::from)
    }

    async fn publish_batch_committed(
        &self,
        batch: BusOutboundEventBatch,
        trace: TraceContextRef,
    ) -> Result<PublishBatchReceipt, ApplicationError> {
        for event in &batch.items {
            self.validate_event(event)?;
        }

        self.deps
            .publisher
            .publish_batch(batch, trace)
            .await
            .map_err(ApplicationError::from)
    }
}

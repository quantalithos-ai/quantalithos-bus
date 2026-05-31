//! In-memory outbound publisher sink and evidence adapter.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bus_application::{
    OutboxPublisherPort, PublishBatchReceipt, PublishEvidenceRecord, PublishEvidenceStatus,
    PublishReceipt, PublisherPortError,
};
use bus_contracts::events::{BusOutboundEvent, BusOutboundEventBatch};
use bus_contracts::metadata::{Timestamp, TraceContextRef};

use crate::observability::{SharedTapOutputSink, TapOutputRecord};

/// One published event captured by the fake sink.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedEventRecord {
    /// The committed outbound event.
    pub event: BusOutboundEvent,
    /// The trace reference supplied to the publisher call.
    pub trace_ref: TraceContextRef,
    /// The committed publish receipt.
    pub receipt: PublishReceipt,
}

/// The shared fake sink used by in-memory publisher tests.
pub type SharedPublishedEventSink = Arc<Mutex<Vec<PublishedEventRecord>>>;

#[derive(Default)]
struct PublisherMemoryState {
    receipts: HashMap<(String, String, String), PublishReceipt>,
    published: Vec<PublishedEventRecord>,
    evidence: Vec<PublishEvidenceRecord>,
    fail_next: Option<PublisherPortError>,
}

/// In-memory outbound publisher adapter.
#[derive(Clone)]
pub struct InMemoryOutboxPublisherAdapter {
    state: Arc<Mutex<PublisherMemoryState>>,
    published_at: Timestamp,
}

impl InMemoryOutboxPublisherAdapter {
    /// Creates a new in-memory publisher adapter.
    pub fn new(published_at: Timestamp) -> Self {
        Self {
            state: Arc::new(Mutex::new(PublisherMemoryState::default())),
            published_at,
        }
    }

    /// Returns the captured published events.
    pub fn published_events(&self) -> Vec<PublishedEventRecord> {
        self.state
            .lock()
            .expect("publisher state lock poisoned")
            .published
            .clone()
    }

    /// Returns the persisted publish evidence records.
    pub fn publish_evidence(&self) -> Vec<PublishEvidenceRecord> {
        self.state
            .lock()
            .expect("publisher state lock poisoned")
            .evidence
            .clone()
    }

    /// Fails the next publish call with the provided error.
    pub fn fail_next_publish(&self, error: PublisherPortError) {
        self.state
            .lock()
            .expect("publisher state lock poisoned")
            .fail_next = Some(error);
    }

    /// Returns the shared sink contents for fake-consumer assertions.
    pub fn shared_sink(&self) -> SharedPublishedEventSink {
        Arc::new(Mutex::new(self.published_events()))
    }

    /// Returns the tap-output records exposed to fake observability consumers.
    pub fn tap_outputs(&self) -> Vec<TapOutputRecord> {
        self.published_events()
            .into_iter()
            .map(|record| TapOutputRecord {
                event: record.event,
                trace_ref: record.trace_ref,
                published_at: record.receipt.published_at,
            })
            .collect()
    }

    /// Returns the shared tap-output sink for fake consumer assertions.
    pub fn shared_tap_sink(&self) -> SharedTapOutputSink {
        Arc::new(Mutex::new(self.tap_outputs()))
    }

    fn evidence_status(error: &PublisherPortError) -> PublishEvidenceStatus {
        match error {
            PublisherPortError::RetryableFailure => PublishEvidenceStatus::RetryableFailed,
            PublisherPortError::SchemaViolation
            | PublisherPortError::BoundaryViolation
            | PublisherPortError::Duplicate => PublishEvidenceStatus::Rejected,
        }
    }

    fn evidence_code(error: &PublisherPortError) -> &'static str {
        match error {
            PublisherPortError::RetryableFailure => "publisher.retryable_failure",
            PublisherPortError::SchemaViolation => "publisher.schema_violation",
            PublisherPortError::BoundaryViolation => "publisher.boundary_violation",
            PublisherPortError::Duplicate => "publisher.duplicate",
        }
    }

    fn record_evidence(
        state: &mut PublisherMemoryState,
        event: &BusOutboundEvent,
        status: PublishEvidenceStatus,
        error_code: Option<&'static str>,
    ) {
        state.evidence.push(PublishEvidenceRecord {
            evidence_ref: format!("publish_evidence_{}", sanitize(event.event_id.as_str())),
            event_id: event.event_id.clone(),
            topic: event.topic().to_owned(),
            record_ref: event.record_ref(),
            schema_version: event.schema_version().to_owned(),
            status,
            error_code,
        });
    }
}

impl OutboxPublisherPort for InMemoryOutboxPublisherAdapter {
    async fn publish(
        &self,
        event: BusOutboundEvent,
        trace: TraceContextRef,
    ) -> Result<PublishReceipt, PublisherPortError> {
        let mut state = self.state.lock().expect("publisher state lock poisoned");
        let key = (
            event.event_id.as_str().to_owned(),
            event.record_ref(),
            event.schema_version().to_owned(),
        );

        if let Some(receipt) = state.receipts.get(&key) {
            let mut receipt = receipt.clone();
            receipt.duplicate = true;
            return Ok(receipt);
        }

        if let Some(error) = state.fail_next.take() {
            Self::record_evidence(
                &mut state,
                &event,
                Self::evidence_status(&error),
                Some(Self::evidence_code(&error)),
            );
            return Err(error);
        }

        let receipt = PublishReceipt {
            receipt_ref: format!("publish_receipt_{}", sanitize(event.event_id.as_str())),
            event_id: event.event_id.clone(),
            topic: event.topic().to_owned(),
            published_at: self.published_at.clone(),
            duplicate: false,
        };
        state.receipts.insert(key, receipt.clone());
        state.published.push(PublishedEventRecord {
            event: event.clone(),
            trace_ref: trace,
            receipt: receipt.clone(),
        });
        Self::record_evidence(&mut state, &event, PublishEvidenceStatus::Published, None);

        Ok(receipt)
    }

    async fn publish_batch(
        &self,
        batch: BusOutboundEventBatch,
        trace: TraceContextRef,
    ) -> Result<PublishBatchReceipt, PublisherPortError> {
        let mut receipts = Vec::with_capacity(batch.items.len());
        for event in batch.items {
            receipts.push(self.publish(event, trace.clone()).await?);
        }

        Ok(PublishBatchReceipt { receipts })
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

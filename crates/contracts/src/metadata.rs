//! Shared metadata and value objects for bus protocol DTOs.

use std::fmt;

use serde::{Deserialize, Serialize};

pub use core_contracts::actor::{ActorContext, ActorKind, ActorRef, RequestOrigin};
pub use core_contracts::metadata::{
    CommandMetadata, IdempotencyKey, JobRunId, PageRequest, PageToken, QueryConsistency,
    QueryMetadata, RequestId, RequestMetadata, Timestamp, TraceId, Version,
};

/// Reuses the shared core trace identifier as the bus trace reference.
pub type TraceContextRef = TraceId;

macro_rules! string_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates a new wrapped string value.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Returns the wrapped string value.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

macro_rules! numeric_newtype {
    ($name:ident, $inner:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name($inner);

        impl $name {
            /// Creates a new wrapped numeric value.
            pub fn new(value: $inner) -> Self {
                Self(value)
            }

            /// Returns the wrapped numeric value.
            pub fn get(self) -> $inner {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }
    };
}

string_newtype!(AuditRef, "A reference to a committed bus audit entry.");
string_newtype!(
    BackendCapabilityId,
    "A stable backend capability identifier."
);
string_newtype!(
    BackendDeliveryRef,
    "A normalized backend delivery reference."
);
string_newtype!(BackendId, "A stable backend identifier.");
string_newtype!(
    BackendProfileRef,
    "A backend profile reference without secrets."
);
string_newtype!(
    CapabilityVersion,
    "A backend capability version identifier."
);
string_newtype!(
    CommittedOutboxFactRef,
    "A reference to a committed upstream outbox fact."
);
string_newtype!(
    ConsumerMarker,
    "A stable consumer marker for source acknowledgements."
);
string_newtype!(CoreEventRef, "A reference to an L0-core event contract.");
string_newtype!(
    CoreEventEnvelopeRef,
    "A reference to a committed L0-core event envelope."
);
string_newtype!(DeliveryAttemptId, "A stable delivery attempt identifier.");
string_newtype!(
    DeliveryAttemptRef,
    "A stable reference to a delivery attempt."
);
string_newtype!(
    DeliveryHistoryId,
    "A stable delivery history entry identifier."
);
string_newtype!(DeliveryId, "A stable delivery identifier.");
string_newtype!(
    DeliveryScanCursor,
    "A cursor for schedulable delivery scans."
);
string_newtype!(
    DeliveryTransitionRuleRef,
    "A reference to the delivery transition rule set."
);
string_newtype!(EventId, "A stable inbound event identifier.");
string_newtype!(EventSourceRef, "A stable inbound event source reference.");
string_newtype!(FailureReason, "A stable delivery failure reason.");
string_newtype!(FeedbackId, "A stable feedback identifier.");
string_newtype!(
    ForbiddenBodyPolicyRef,
    "A reference to the payload body boundary policy."
);
string_newtype!(HistoryReason, "A stable delivery history reason.");
string_newtype!(
    OutboxFactRef,
    "A reference to a committed upstream outbox fact."
);
string_newtype!(OutboxCursor, "A cursor for committed outbox fact scans.");
string_newtype!(PayloadDigest, "A digest for the referenced payload.");
string_newtype!(PayloadRef, "A reference to an external payload body.");
string_newtype!(
    PublicationAcceptanceId,
    "A stable publication acceptance fact identifier."
);
string_newtype!(PublicationId, "A stable publication material identifier.");
string_newtype!(
    RejectionReasonRef,
    "A stable reference to a publication rejection reason code."
);
string_newtype!(SourceRecordRef, "A stable source record reference.");
string_newtype!(SourceSystem, "A stable source system identifier.");
string_newtype!(SubscriberRef, "A stable subscriber identifier.");
string_newtype!(
    TransportSemanticId,
    "A stable transport semantic identifier."
);

numeric_newtype!(
    AttemptCount,
    u32,
    "The number of attempts recorded for a delivery."
);
numeric_newtype!(AttemptNo, u32, "A one-based delivery attempt number.");
numeric_newtype!(
    PageLimit,
    u32,
    "A bounded page size for source and repository scans."
);

impl Default for AttemptCount {
    fn default() -> Self {
        Self::new(0)
    }
}

impl AttemptCount {
    /// Returns the next attempt number for the current count.
    pub fn next_attempt_no(self) -> AttemptNo {
        AttemptNo::new(self.get() + 1)
    }

    /// Returns the incremented count.
    pub fn increment(self) -> Self {
        Self::new(self.get() + 1)
    }
}

impl ConsumerMarker {
    /// Returns the stable marker used by the bus source consumer.
    pub fn bus() -> Self {
        Self::new("bus")
    }
}

impl HistoryReason {
    /// Returns the stable reason for a dispatch-start transition.
    pub fn dispatching_started() -> Self {
        Self::new("dispatching_started")
    }

    /// Returns the stable reason for a delivered transition.
    pub fn delivery_arrived() -> Self {
        Self::new("delivery_arrived")
    }

    /// Returns the stable reason for a failed transition.
    pub fn delivery_failed() -> Self {
        Self::new("delivery_failed")
    }
}

impl FailureReason {
    /// Returns the stable reason for an unavailable backend.
    pub fn backend_unavailable() -> Self {
        Self::new("backend_unavailable")
    }

    /// Returns the stable reason for a normalized backend dispatch failure.
    pub fn dispatch_failed() -> Self {
        Self::new("dispatch_failed")
    }
}

impl Default for OutboxCursor {
    fn default() -> Self {
        Self::origin()
    }
}

impl OutboxCursor {
    /// Returns the stable origin cursor for a fresh source scan.
    pub fn origin() -> Self {
        Self::new("origin")
    }
}

impl From<CommittedOutboxFactRef> for OutboxFactRef {
    fn from(value: CommittedOutboxFactRef) -> Self {
        Self::new(value.as_str())
    }
}

/// The supported payload reference kinds for publication acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKind {
    /// The payload is carried as an external artifact reference.
    ArtifactRef,
}

/// The platform delivery mode requested by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    /// P0 only accepts at-least-once delivery requests.
    AtLeastOnce,
}

/// The publication acceptance lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationAcceptanceStatus {
    /// The acceptance decision has not been finalized yet.
    Pending,
    /// The publication material was accepted by the bus.
    Accepted,
    /// The publication material was rejected by the bus.
    Rejected,
}

/// The bus-owned delivery lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// The delivery is scheduled but not yet dispatching.
    Scheduled,
    /// The delivery is currently dispatching through a backend capability.
    Dispatching,
    /// The delivery reached the subscriber boundary and awaits feedback.
    Delivered,
    /// The delivery failed during dispatch or feedback handling.
    Failed,
    /// The delivery entered the dead-letter path.
    DeadLettered,
    /// The delivery was completed by an acknowledged feedback result.
    Completed,
}

/// The supported transport backend kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// The in-memory backend used by the P0 verification path.
    InMemory,
}

/// The normalized backend dispatch status used by attempts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendDeliveryStatus {
    /// The backend reported that the delivery was handed off successfully.
    Delivered,
    /// The backend reported that the delivery failed.
    Failed,
}

/// The consistency marker returned by read-only delivery views.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyMarker {
    /// The returned view reflects committed truth.
    Committed,
}

/// The source that triggered a one-off operations job.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobTriggerSource {
    /// The job was started by a scheduler.
    Scheduler,
    /// The job was started from a CLI or operator action.
    Cli,
}

/// Shared metadata supplied to operations jobs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobMetadata {
    /// The unique job run identifier.
    pub job_run_id: JobRunId,
    /// The trace reference attached to the job run.
    pub trace_ref: TraceContextRef,
    /// The trigger source that started the run.
    pub trigger_source: JobTriggerSource,
}

/// Shared metadata supplied to inbound event consumers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventMetadata {
    /// The trace reference attached to the inbound event.
    pub trace_ref: TraceContextRef,
}

impl EventMetadata {
    /// Builds consumer metadata from an operations job context.
    pub fn from_job(meta: JobMetadata) -> Self {
        Self {
            trace_ref: meta.trace_ref,
        }
    }
}

/// The logical target scope requested by a publication command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetScope {
    /// The project or namespace that owns the target topic.
    pub project_id: String,
    /// The logical topic name requested by the caller.
    pub topic: String,
}

/// The subscriber scope carried by transport semantic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriberScope {
    /// The project or namespace that owns the subscriber topic.
    pub project_id: String,
    /// The stable subscriber topic.
    pub topic: String,
}

impl From<TargetScope> for SubscriberScope {
    fn from(value: TargetScope) -> Self {
        Self {
            project_id: value.project_id,
            topic: value.topic,
        }
    }
}

/// The backend capability reference attached to transport semantic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendCapabilityRef {
    /// The stable capability identifier.
    pub capability_id: BackendCapabilityId,
    /// The backend kind.
    pub backend_kind: BackendKind,
    /// The backend profile reference.
    pub profile_ref: BackendProfileRef,
    /// The backend capability version.
    pub capability_version: CapabilityVersion,
}

impl BackendCapabilityRef {
    /// Builds a backend capability reference from a profile and capability kind.
    pub fn from_profile(
        profile_ref: BackendProfileRef,
        backend_kind: BackendKind,
        capability_version: CapabilityVersion,
    ) -> Self {
        let capability_id = BackendCapabilityId::new(format!(
            "capability_{}_{}_{}",
            sanitize(profile_ref.as_str()),
            match backend_kind {
                BackendKind::InMemory => "in_memory",
            },
            sanitize(capability_version.as_str())
        ));

        Self {
            capability_id,
            backend_kind,
            profile_ref,
            capability_version,
        }
    }

    /// Returns whether the reference points to the provided backend kind.
    pub fn is_kind(&self, backend_kind: BackendKind) -> bool {
        self.backend_kind == backend_kind
    }

    /// Returns whether the reference points to the provided backend profile.
    pub fn matches_profile(&self, profile_ref: BackendProfileRef) -> bool {
        self.profile_ref == profile_ref
    }
}

/// The normalized backend dispatch result stored on a delivery attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendDeliveryResult {
    /// The normalized backend delivery status.
    pub status: BackendDeliveryStatus,
    /// The normalized backend delivery reference, if one exists.
    pub backend_ref: Option<BackendDeliveryRef>,
}

impl BackendDeliveryResult {
    /// Builds a normalized delivered result.
    pub fn delivered(backend_ref: Option<BackendDeliveryRef>) -> Self {
        Self {
            status: BackendDeliveryStatus::Delivered,
            backend_ref,
        }
    }

    /// Builds a normalized failed result.
    pub fn failed(backend_ref: Option<BackendDeliveryRef>) -> Self {
        Self {
            status: BackendDeliveryStatus::Failed,
            backend_ref,
        }
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

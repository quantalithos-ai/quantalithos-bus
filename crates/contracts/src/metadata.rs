//! Shared metadata and value objects for bus protocol DTOs.

use std::fmt;

use serde::{Deserialize, Serialize};

pub use core_contracts::actor::{ActorContext, ActorKind, ActorRef, RequestOrigin};
pub use core_contracts::metadata::{
    CommandMetadata, IdempotencyKey, RequestId, RequestMetadata, Timestamp, TraceId, Version,
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

string_newtype!(AuditRef, "A reference to a committed bus audit entry.");
string_newtype!(CoreEventRef, "A reference to an L0-core event contract.");
string_newtype!(
    ForbiddenBodyPolicyRef,
    "A reference to the payload body boundary policy."
);
string_newtype!(
    OutboxFactRef,
    "A reference to a committed upstream outbox fact."
);
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

/// The logical target scope requested by a publication command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetScope {
    /// The project or namespace that owns the target topic.
    pub project_id: String,
    /// The logical topic name requested by the caller.
    pub topic: String,
}

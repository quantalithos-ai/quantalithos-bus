//! Read-only DTOs returned by bus query APIs.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    ConsistencyMarker, DeliveryAttemptId, DeliveryId, DeliveryStatus, FeedbackId, PublicationId,
};

/// Returns the current committed status of a delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryStatusView {
    /// The delivery identifier.
    pub delivery_id: DeliveryId,
    /// The associated publication identifier.
    pub publication_id: PublicationId,
    /// The current delivery lifecycle state.
    pub delivery_status: DeliveryStatus,
    /// The current attempt identifier, if an attempt exists.
    pub current_attempt_id: Option<DeliveryAttemptId>,
    /// The last feedback identifier, if feedback has already been recorded.
    pub last_feedback_id: Option<FeedbackId>,
    /// The consistency marker for the returned view.
    pub consistency_marker: ConsistencyMarker,
}

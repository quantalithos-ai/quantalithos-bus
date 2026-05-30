//! Query DTOs for bus read-only delivery APIs.

use serde::{Deserialize, Serialize};

use crate::metadata::DeliveryId;

/// Queries the current state of a delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetDeliveryStatusQuery {
    /// The target delivery identifier.
    pub delivery_id: DeliveryId,
}

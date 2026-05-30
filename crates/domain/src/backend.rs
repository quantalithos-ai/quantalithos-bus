//! Backend capability policies for transport semantic mapping.

use bus_contracts::metadata::{BackendCapabilityRef, BackendKind, DeliveryMode};

use crate::publication::TransportSemantic;

/// Validates whether a backend capability may carry a transport semantic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendCapabilityPolicy {
    /// The capability reference being validated.
    pub capability_ref: BackendCapabilityRef,
}

impl BackendCapabilityPolicy {
    /// Creates a backend capability policy from a capability reference.
    pub fn from_capability(capability_ref: BackendCapabilityRef) -> Self {
        Self { capability_ref }
    }

    /// Returns whether the semantic may be mapped to the provided capability reference.
    pub fn allows_mapping(
        &self,
        semantic: TransportSemantic,
        capability_ref: BackendCapabilityRef,
    ) -> bool {
        self.capability_ref == capability_ref
            && semantic.uses_backend(capability_ref)
            && matches!(self.capability_ref.backend_kind, BackendKind::InMemory)
            && matches!(semantic.delivery_mode, DeliveryMode::AtLeastOnce)
            && !self.rejects_raw_backend_leak(semantic)
    }

    /// Returns whether backend-private data leaked into the semantic boundary.
    pub fn rejects_raw_backend_leak(&self, semantic: TransportSemantic) -> bool {
        looks_like_private_backend_payload(semantic.backend_capability_ref.profile_ref.as_str())
            || looks_like_private_backend_payload(
                semantic.backend_capability_ref.capability_id.as_str(),
            )
    }
}

fn looks_like_private_backend_payload(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();

    normalized.contains("://")
        || normalized.contains("password")
        || normalized.contains("secret")
        || normalized.contains("token")
        || normalized.contains("credential")
        || normalized.contains("@")
}

#[cfg(test)]
mod tests {
    use bus_contracts::fixtures::{
        BackendFixtureBuilder, PublicationFixtureBuilder, TestRunBuilder,
    };
    use bus_contracts::metadata::{
        BackendCapabilityRef, BackendKind, CapabilityVersion, DeliveryMode, SubscriberScope,
    };

    use super::*;
    use crate::publication::{PublicationMaterial, TransportSemantic};

    fn semantic() -> TransportSemantic {
        let run = TestRunBuilder::new("backend-policy-001").build();
        let actor = run.actor.clone();
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            actor,
            run.metadata,
        )
        .expect("fixture should create valid material");

        TransportSemantic::derive(
            material,
            backend_builder.in_memory_capability(),
            SubscriberScope {
                project_id: format!("project_{}", run.run_id),
                topic: format!("workitem.events.{}", run.run_id),
            },
        )
        .expect("fixture should derive semantic")
    }

    #[test]
    fn backend_capability_policy_allows_in_memory_mapping() {
        let semantic = semantic();
        let capability_ref = semantic.backend_capability_ref.clone();
        let policy = BackendCapabilityPolicy::from_capability(capability_ref.clone());

        assert!(policy.allows_mapping(semantic, capability_ref));
    }

    #[test]
    fn backend_capability_policy_rejects_mismatched_capability() {
        let semantic = semantic();
        let policy =
            BackendCapabilityPolicy::from_capability(semantic.backend_capability_ref.clone());
        let mismatched = BackendCapabilityRef::from_profile(
            "profile_mismatch".into(),
            BackendKind::InMemory,
            CapabilityVersion::new("v1"),
        );

        assert!(!policy.allows_mapping(semantic, mismatched));
    }

    #[test]
    fn backend_capability_policy_rejects_private_backend_data() {
        let run = TestRunBuilder::new("backend-policy-002").build();
        let backend_builder = BackendFixtureBuilder::new(run);
        let semantic = TransportSemantic {
            semantic_id: "semantic_private".into(),
            publication_id: "pub_private".into(),
            delivery_mode: DeliveryMode::AtLeastOnce,
            target_scope: SubscriberScope {
                project_id: "project_private".to_owned(),
                topic: "topic.private".to_owned(),
            },
            backend_capability_ref: backend_builder.tainted_capability(),
        };
        let policy =
            BackendCapabilityPolicy::from_capability(semantic.backend_capability_ref.clone());

        assert!(policy.rejects_raw_backend_leak(semantic));
    }
}

//! Publication acceptance domain objects and payload boundary policies.

use bus_contracts::commands::AcceptPublicationCommand;
use bus_contracts::metadata::{
    ActorContext, AuditRef, BackendCapabilityRef, CommandMetadata, CoreEventRef, DeliveryMode,
    ForbiddenBodyPolicyRef, OutboxFactRef, PayloadDigest, PayloadKind, PayloadRef,
    PublicationAcceptanceId, PublicationAcceptanceStatus, PublicationId, RejectionReasonRef,
    SourceRecordRef, SourceSystem, SubscriberScope, TargetScope, Timestamp, TraceContextRef,
    TransportSemanticId,
};

use crate::backend::BackendCapabilityPolicy;
use crate::errors::DomainError;

/// Publication material references carried into the bus.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationMaterial {
    /// The stable publication material identifier.
    pub publication_id: PublicationId,
    /// The upstream source system that produced the material.
    pub source_system: SourceSystem,
    /// The stable upstream source record reference.
    pub source_record_ref: SourceRecordRef,
    /// The referenced L0-core event contract.
    pub core_event_ref: CoreEventRef,
    /// The referenced payload body location.
    pub payload_ref: PayloadRef,
    /// The declared payload reference kind.
    pub payload_kind: PayloadKind,
    /// The digest for the referenced payload body.
    pub payload_digest: PayloadDigest,
    /// The requested platform delivery mode.
    pub delivery_mode: DeliveryMode,
    /// The logical target scope carried by the publication command.
    pub target_scope: TargetScope,
    /// The optional committed upstream outbox fact reference.
    pub outbox_fact_ref: Option<OutboxFactRef>,
    /// The actor context supplied by the trusted outer boundary.
    pub actor: ActorContext,
    /// The trace reference that must remain attached to audit and events.
    pub trace_ref: TraceContextRef,
}

impl PublicationMaterial {
    /// Builds publication material from an `AcceptPublicationCommand`.
    pub fn from_accept_publication_command(
        command: AcceptPublicationCommand,
        actor: ActorContext,
        meta: CommandMetadata,
    ) -> Result<Self, DomainError> {
        if command.source_system.as_str().trim().is_empty() {
            return Err(DomainError::InvalidPublicationMaterial("source_system"));
        }
        if command.source_record_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidPublicationMaterial("source_record_ref"));
        }
        if command.core_event_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidPublicationMaterial("core_event_ref"));
        }
        if command.payload_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidPublicationMaterial("payload_ref"));
        }
        if command.payload_digest.as_str().trim().is_empty() {
            return Err(DomainError::InvalidPublicationMaterial("payload_digest"));
        }
        if command.target_scope.project_id.trim().is_empty() {
            return Err(DomainError::InvalidPublicationMaterial(
                "target_scope.project_id",
            ));
        }
        if command.target_scope.topic.trim().is_empty() {
            return Err(DomainError::InvalidPublicationMaterial(
                "target_scope.topic",
            ));
        }

        let publication_id = PublicationId::new(format!(
            "pub_{}_{}",
            sanitize(command.source_system.as_str()),
            sanitize(command.source_record_ref.as_str())
        ));

        Ok(Self {
            publication_id,
            source_system: command.source_system,
            source_record_ref: command.source_record_ref,
            core_event_ref: command.core_event_ref,
            payload_ref: command.payload_ref,
            payload_kind: command.payload_kind,
            payload_digest: command.payload_digest,
            delivery_mode: command.delivery_mode,
            target_scope: command.target_scope,
            outbox_fact_ref: None,
            actor,
            trace_ref: meta.request.trace_id,
        })
    }

    /// Returns whether a core contract reference is present.
    pub fn has_core_contract(&self) -> bool {
        !self.core_event_ref.as_str().trim().is_empty()
    }

    /// Returns whether the material only carries a payload reference.
    pub fn has_payload_reference(&self) -> bool {
        !self.payload_ref.as_str().trim().is_empty()
    }

    /// Returns whether the material originated from outbox relay input.
    pub fn is_from_outbox(&self) -> bool {
        self.outbox_fact_ref.is_some()
    }
}

/// Platform-level transport semantic derived from accepted publication material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportSemantic {
    /// The stable transport semantic identifier.
    pub semantic_id: TransportSemanticId,
    /// The associated publication identifier.
    pub publication_id: PublicationId,
    /// The platform delivery mode.
    pub delivery_mode: DeliveryMode,
    /// The normalized subscriber scope.
    pub target_scope: SubscriberScope,
    /// The selected backend capability reference.
    pub backend_capability_ref: BackendCapabilityRef,
}

impl TransportSemantic {
    /// Derives transport semantic from accepted publication material and backend capability.
    pub fn derive(
        material: PublicationMaterial,
        capability_ref: BackendCapabilityRef,
        target_scope: SubscriberScope,
    ) -> Result<Self, DomainError> {
        if target_scope.project_id.trim().is_empty() {
            return Err(DomainError::InvalidSubscriberScope("project_id"));
        }
        if target_scope.topic.trim().is_empty() {
            return Err(DomainError::InvalidSubscriberScope("topic"));
        }

        let declared_scope = SubscriberScope::from(material.target_scope.clone());
        if declared_scope != target_scope {
            return Err(DomainError::TargetScopeMismatch);
        }

        let semantic = Self {
            semantic_id: TransportSemanticId::new(format!(
                "semantic_{}_{}",
                sanitize(material.publication_id.as_str()),
                sanitize(capability_ref.capability_id.as_str())
            )),
            publication_id: material.publication_id,
            delivery_mode: material.delivery_mode,
            target_scope,
            backend_capability_ref: capability_ref.clone(),
        };

        let policy = BackendCapabilityPolicy::from_capability(capability_ref.clone());
        if policy.rejects_raw_backend_leak(semantic.clone()) {
            return Err(DomainError::BackendPrivateFieldLeak);
        }
        if !policy.allows_mapping(semantic.clone(), capability_ref) {
            return Err(DomainError::BackendCapabilityMappingRejected);
        }

        Ok(semantic)
    }

    /// Returns whether the semantic requires a durable delivery record.
    pub fn requires_durable_record(&self) -> bool {
        matches!(self.delivery_mode, DeliveryMode::AtLeastOnce)
    }

    /// Returns whether the semantic matches the provided subscriber scope.
    pub fn matches_scope(&self, scope: SubscriberScope) -> bool {
        self.target_scope == scope
    }

    /// Returns whether the semantic uses the provided backend capability.
    pub fn uses_backend(&self, capability_ref: BackendCapabilityRef) -> bool {
        self.backend_capability_ref == capability_ref
    }
}

/// The stable reason carried by a rejected publication acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationRejectReason {
    /// The publication command did not carry a valid core event reference.
    MissingCoreEventRef,
    /// The publication command attempted to inline forbidden payload body content.
    PayloadBoundaryViolation,
}

impl PublicationRejectReason {
    /// Returns the stable protocol reason reference for this rejection.
    pub fn reason_ref(self) -> RejectionReasonRef {
        match self {
            Self::MissingCoreEventRef => {
                RejectionReasonRef::new("validation.core_event_ref_missing")
            }
            Self::PayloadBoundaryViolation => {
                RejectionReasonRef::new("boundary.payload_body_rejected")
            }
        }
    }
}

/// The publication acceptance fact stored by the bus.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationAcceptance {
    /// The stable acceptance fact identifier.
    pub acceptance_id: PublicationAcceptanceId,
    /// The associated publication material identifier.
    pub publication_id: PublicationId,
    /// The current acceptance status.
    pub status: PublicationAcceptanceStatus,
    /// The rejection reason for rejected acceptances.
    pub reject_reason: Option<PublicationRejectReason>,
    /// The acceptance timestamp for accepted publications.
    pub accepted_at: Option<Timestamp>,
    /// The audit reference bound to the terminal decision.
    pub decision_audit_ref: Option<AuditRef>,
}

impl PublicationAcceptance {
    /// Creates a new pending publication acceptance fact.
    pub fn start_pending(
        material: PublicationMaterial,
        _actor: ActorContext,
    ) -> Result<Self, DomainError> {
        if !material.has_core_contract() {
            return Err(DomainError::InvalidPublicationMaterial("core_event_ref"));
        }
        if !material.has_payload_reference() {
            return Err(DomainError::InvalidPublicationMaterial("payload_ref"));
        }

        Ok(Self {
            acceptance_id: PublicationAcceptanceId::new(format!(
                "acceptance_{}",
                sanitize(material.publication_id.as_str())
            )),
            publication_id: material.publication_id,
            status: PublicationAcceptanceStatus::Pending,
            reject_reason: None,
            accepted_at: None,
            decision_audit_ref: None,
        })
    }

    /// Marks the pending acceptance as accepted.
    pub fn accept(
        &mut self,
        _actor: ActorContext,
        occurred_at: Timestamp,
        audit_ref: AuditRef,
    ) -> Result<(), DomainError> {
        match self.status {
            PublicationAcceptanceStatus::Pending => {
                self.status = PublicationAcceptanceStatus::Accepted;
                self.reject_reason = None;
                self.accepted_at = Some(occurred_at);
                self.decision_audit_ref = Some(audit_ref);
                Ok(())
            }
            PublicationAcceptanceStatus::Accepted => Err(DomainError::InvalidStateTransition {
                from: PublicationAcceptanceStatus::Accepted,
                to: PublicationAcceptanceStatus::Accepted,
            }),
            PublicationAcceptanceStatus::Rejected => Err(DomainError::InvalidStateTransition {
                from: PublicationAcceptanceStatus::Rejected,
                to: PublicationAcceptanceStatus::Accepted,
            }),
        }
    }

    /// Marks the pending acceptance as rejected.
    pub fn reject(
        &mut self,
        reason: PublicationRejectReason,
        _actor: ActorContext,
        audit_ref: AuditRef,
    ) -> Result<(), DomainError> {
        match self.status {
            PublicationAcceptanceStatus::Pending => {
                self.status = PublicationAcceptanceStatus::Rejected;
                self.reject_reason = Some(reason);
                self.accepted_at = None;
                self.decision_audit_ref = Some(audit_ref);
                Ok(())
            }
            PublicationAcceptanceStatus::Accepted => Err(DomainError::InvalidStateTransition {
                from: PublicationAcceptanceStatus::Accepted,
                to: PublicationAcceptanceStatus::Rejected,
            }),
            PublicationAcceptanceStatus::Rejected => Err(DomainError::InvalidStateTransition {
                from: PublicationAcceptanceStatus::Rejected,
                to: PublicationAcceptanceStatus::Rejected,
            }),
        }
    }

    /// Returns whether the publication is accepted for downstream delivery work.
    pub fn is_accepted(&self) -> bool {
        self.status == PublicationAcceptanceStatus::Accepted
    }
}

/// A boundary policy that rejects forbidden body material before it reaches truth.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadBoundaryGuard {
    /// The policy reference used by the current boundary check.
    pub forbidden_body_policy_ref: ForbiddenBodyPolicyRef,
}

impl PayloadBoundaryGuard {
    /// Creates the default bus payload body guard.
    pub fn default_for_bus() -> Self {
        Self {
            forbidden_body_policy_ref: ForbiddenBodyPolicyRef::new(
                "policy.bus.default_forbidden_body",
            ),
        }
    }

    /// Returns whether the publication material carries forbidden body content.
    pub fn rejects_body(&self, material: PublicationMaterial) -> bool {
        !self.allows_reference(material.payload_ref)
    }

    /// Returns whether the payload reference looks like an external reference rather than body.
    pub fn allows_reference(&self, payload_ref: PayloadRef) -> bool {
        let value = payload_ref.as_str().trim();
        if value.is_empty() {
            return false;
        }

        !value.starts_with('{')
            && !value.starts_with('[')
            && !value.contains('\n')
            && !value.contains('\r')
            && !value.contains("\"secret\"")
            && !value.contains("\"payload\"")
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use bus_contracts::fixtures::{
        BackendFixtureBuilder, PublicationFixtureBuilder, TestRunBuilder,
    };
    use bus_contracts::metadata::{
        AuditRef, PublicationAcceptanceStatus, SubscriberScope, Timestamp,
    };

    use super::*;

    fn valid_material() -> (PublicationMaterial, ActorContext) {
        let run = TestRunBuilder::new("pub-domain-001").build();
        let actor = run.actor.clone();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            builder.valid_material(),
            actor.clone(),
            run.metadata,
        )
        .expect("fixture should build valid publication material");

        (material, actor)
    }

    #[test]
    fn publication_material_from_command_preserves_reference_only_fields() {
        let (material, _) = valid_material();

        assert!(material.has_core_contract());
        assert!(material.has_payload_reference());
        assert!(!material.is_from_outbox());
        assert!(material.trace_ref.as_str().contains("trace-pub-domain-001"));
        assert!(material.source_system.as_str().contains("l2-process"));
        assert!(
            material
                .source_record_ref
                .as_str()
                .contains("process_event")
        );
        assert_eq!(material.delivery_mode, DeliveryMode::AtLeastOnce);
    }

    #[test]
    fn publication_acceptance_starts_pending() {
        let (material, actor) = valid_material();
        let acceptance =
            PublicationAcceptance::start_pending(material.clone(), actor).expect("pending start");

        assert_eq!(acceptance.publication_id, material.publication_id);
        assert_eq!(acceptance.status, PublicationAcceptanceStatus::Pending);
        assert!(acceptance.reject_reason.is_none());
        assert!(acceptance.accepted_at.is_none());
        assert!(acceptance.decision_audit_ref.is_none());
    }

    #[test]
    fn publication_acceptance_accepts_from_pending() {
        let (material, actor) = valid_material();
        let mut acceptance =
            PublicationAcceptance::start_pending(material, actor.clone()).expect("pending start");

        acceptance
            .accept(
                actor,
                Timestamp::new("2026-05-30T00:00:01Z"),
                AuditRef::new("audit-accepted"),
            )
            .expect("accept should succeed");

        assert_eq!(acceptance.status, PublicationAcceptanceStatus::Accepted);
        assert!(acceptance.is_accepted());
        assert!(acceptance.reject_reason.is_none());
        assert_eq!(
            acceptance.accepted_at,
            Some(Timestamp::new("2026-05-30T00:00:01Z"))
        );
        assert_eq!(
            acceptance.decision_audit_ref,
            Some(AuditRef::new("audit-accepted"))
        );
    }

    #[test]
    fn publication_acceptance_rejects_from_pending() {
        let (material, actor) = valid_material();
        let mut acceptance =
            PublicationAcceptance::start_pending(material, actor.clone()).expect("pending start");

        acceptance
            .reject(
                PublicationRejectReason::PayloadBoundaryViolation,
                actor,
                AuditRef::new("audit-rejected"),
            )
            .expect("reject should succeed");

        assert_eq!(acceptance.status, PublicationAcceptanceStatus::Rejected);
        assert!(!acceptance.is_accepted());
        assert_eq!(
            acceptance.reject_reason,
            Some(PublicationRejectReason::PayloadBoundaryViolation)
        );
        assert!(acceptance.accepted_at.is_none());
        assert_eq!(
            acceptance.decision_audit_ref,
            Some(AuditRef::new("audit-rejected"))
        );
    }

    #[test]
    fn publication_acceptance_blocks_terminal_rewrite_after_accept() {
        let (material, actor) = valid_material();
        let mut acceptance =
            PublicationAcceptance::start_pending(material, actor.clone()).expect("pending start");

        acceptance
            .accept(
                actor.clone(),
                Timestamp::new("2026-05-30T00:00:01Z"),
                AuditRef::new("audit-accepted"),
            )
            .expect("accept should succeed");

        let error = acceptance
            .reject(
                PublicationRejectReason::PayloadBoundaryViolation,
                actor,
                AuditRef::new("audit-conflict"),
            )
            .expect_err("accepted state must be terminal");

        assert_eq!(
            error,
            DomainError::InvalidStateTransition {
                from: PublicationAcceptanceStatus::Accepted,
                to: PublicationAcceptanceStatus::Rejected,
            }
        );
    }

    #[test]
    fn publication_acceptance_blocks_terminal_rewrite_after_reject() {
        let (material, actor) = valid_material();
        let mut acceptance =
            PublicationAcceptance::start_pending(material, actor.clone()).expect("pending start");

        acceptance
            .reject(
                PublicationRejectReason::MissingCoreEventRef,
                actor.clone(),
                AuditRef::new("audit-rejected"),
            )
            .expect("reject should succeed");

        let error = acceptance
            .accept(
                actor,
                Timestamp::new("2026-05-30T00:00:01Z"),
                AuditRef::new("audit-conflict"),
            )
            .expect_err("rejected state must be terminal");

        assert_eq!(
            error,
            DomainError::InvalidStateTransition {
                from: PublicationAcceptanceStatus::Rejected,
                to: PublicationAcceptanceStatus::Accepted,
            }
        );
    }

    #[test]
    fn payload_boundary_guard_rejects_inline_body_like_payload_reference() {
        let run = TestRunBuilder::new("pub-domain-002").build();
        let actor = run.actor.clone();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let mut command = builder.valid_material();
        command.payload_ref = PayloadRef::new("{\"payload\":\"secret\"}");

        let material =
            PublicationMaterial::from_accept_publication_command(command, actor, run.metadata)
                .expect("factory still accepts references syntactically");

        let guard = PayloadBoundaryGuard::default_for_bus();

        assert!(guard.rejects_body(material));
    }

    #[test]
    fn transport_semantic_derives_from_accepted_material() {
        let run = TestRunBuilder::new("pub-domain-003").build();
        let actor = run.actor.clone();
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            actor,
            run.metadata,
        )
        .expect("fixture should build valid material");
        let scope = SubscriberScope {
            project_id: format!("project_{}", run.run_id),
            topic: format!("workitem.events.{}", run.run_id),
        };
        let capability_ref = backend_builder.in_memory_capability();

        let semantic = TransportSemantic::derive(material, capability_ref.clone(), scope.clone())
            .expect("semantic should derive");

        assert!(semantic.requires_durable_record());
        assert!(semantic.matches_scope(scope));
        assert!(semantic.uses_backend(capability_ref));
    }

    #[test]
    fn transport_semantic_rejects_scope_mismatch() {
        let run = TestRunBuilder::new("pub-domain-004").build();
        let actor = run.actor.clone();
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            actor,
            run.metadata,
        )
        .expect("fixture should build valid material");

        let error = TransportSemantic::derive(
            material,
            backend_builder.in_memory_capability(),
            SubscriberScope {
                project_id: "different_project".to_owned(),
                topic: "different.topic".to_owned(),
            },
        )
        .expect_err("mismatched scope should be rejected");

        assert_eq!(error, DomainError::TargetScopeMismatch);
    }

    #[test]
    fn transport_semantic_rejects_backend_private_leak() {
        let run = TestRunBuilder::new("pub-domain-005").build();
        let actor = run.actor.clone();
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            actor,
            run.metadata,
        )
        .expect("fixture should build valid material");

        let error = TransportSemantic::derive(
            material,
            backend_builder.tainted_capability(),
            SubscriberScope {
                project_id: format!("project_{}", run.run_id),
                topic: format!("workitem.events.{}", run.run_id),
            },
        )
        .expect_err("tainted capability should be rejected");

        assert_eq!(error, DomainError::BackendPrivateFieldLeak);
    }
}

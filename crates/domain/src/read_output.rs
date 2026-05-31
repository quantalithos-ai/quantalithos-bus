//! Read-only projection objects and guards.

use bus_contracts::metadata::{
    ActorContext, AuditRef, AuthorizationRef, DeadLetterRef, DeliveryId, FailureMaterialId,
    ProjectionVersion, ReadOnlyPolicyRef, TransportViewId,
};

use crate::audit::{BusAuditEntry, PrivilegedAccessRejectionReason, PrivilegedAccessScope};
use crate::delivery::DeliveryRecord;
use crate::errors::DomainError;
use crate::recovery::FailureMaterial;

/// The read-only lifecycle state of one projection record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionStatus {
    /// The projection is being built for the first time.
    Building,
    /// The projection is active and may be queried normally.
    Active,
    /// The projection is visible but stale.
    Stale,
    /// The projection is currently rebuilding.
    Rebuilding,
}

/// One transport-view projection metadata record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportViewProjection {
    /// The stable transport-view identifier.
    pub view_id: TransportViewId,
    /// The source delivery identifier.
    pub delivery_id: DeliveryId,
    /// The current projection state.
    pub status: ProjectionStatus,
    /// The current projection version.
    pub version: ProjectionVersion,
    /// The audit reference that last refreshed the projection.
    pub source_audit_ref: AuditRef,
}

impl TransportViewProjection {
    /// Derives one transport-view projection from committed delivery truth and audit.
    pub fn derive(delivery: DeliveryRecord, audit: BusAuditEntry) -> Result<Self, DomainError> {
        if audit.audit_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidTransportViewProjection(
                "source_audit_ref",
            ));
        }

        Ok(Self {
            view_id: TransportViewId::from_delivery_id(&delivery.delivery_id),
            delivery_id: delivery.delivery_id,
            status: ProjectionStatus::Active,
            version: ProjectionVersion::initial(),
            source_audit_ref: audit.audit_ref,
        })
    }

    /// Marks the projection stale after newer committed truth exists.
    pub fn mark_stale(&mut self, source_audit_ref: AuditRef) -> Result<(), DomainError> {
        if source_audit_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidTransportViewProjection(
                "source_audit_ref",
            ));
        }

        match self.status {
            ProjectionStatus::Building
            | ProjectionStatus::Active
            | ProjectionStatus::Rebuilding => {
                self.status = ProjectionStatus::Stale;
                self.source_audit_ref = source_audit_ref;
                Ok(())
            }
            ProjectionStatus::Stale => Err(DomainError::InvalidProjectionStatusTransition {
                from: ProjectionStatus::Stale,
                to: ProjectionStatus::Stale,
            }),
        }
    }

    /// Returns whether the projection is currently active.
    pub fn is_active(&self) -> bool {
        self.status == ProjectionStatus::Active
    }
}

/// One failure-summary projection metadata record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureSummaryProjection {
    /// The stable failure-summary identifier.
    pub summary_id: bus_contracts::metadata::FailureSummaryId,
    /// The source failure-material identifier.
    pub failure_material_id: FailureMaterialId,
    /// The current projection state.
    pub status: ProjectionStatus,
    /// The optional source dead-letter reference.
    pub source_dead_letter_ref: Option<DeadLetterRef>,
    /// The audit reference that last refreshed the projection.
    pub source_audit_ref: AuditRef,
}

impl FailureSummaryProjection {
    /// Derives one failure-summary projection from committed failure material and audit.
    pub fn derive(material: FailureMaterial, audit: BusAuditEntry) -> Result<Self, DomainError> {
        if audit.audit_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFailureSummaryProjection(
                "source_audit_ref",
            ));
        }

        Ok(Self {
            summary_id: bus_contracts::metadata::FailureSummaryId::from_failure_material_id(
                &material.failure_material_id,
            ),
            failure_material_id: material.failure_material_id,
            status: ProjectionStatus::Active,
            source_dead_letter_ref: material.dead_letter_ref,
            source_audit_ref: audit.audit_ref,
        })
    }

    /// Marks the projection stale after newer committed truth exists.
    pub fn mark_stale(&mut self, source_audit_ref: AuditRef) -> Result<(), DomainError> {
        if source_audit_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFailureSummaryProjection(
                "source_audit_ref",
            ));
        }

        match self.status {
            ProjectionStatus::Building
            | ProjectionStatus::Active
            | ProjectionStatus::Rebuilding => {
                self.status = ProjectionStatus::Stale;
                self.source_audit_ref = source_audit_ref;
                Ok(())
            }
            ProjectionStatus::Stale => Err(DomainError::InvalidProjectionStatusTransition {
                from: ProjectionStatus::Stale,
                to: ProjectionStatus::Stale,
            }),
        }
    }

    /// Returns whether the projection is a governance decision.
    pub fn is_governance_decision(&self) -> bool {
        false
    }
}

/// The write intent evaluated by the read-only output guard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionWriteIntent {
    /// The write only targets one projection record.
    ProjectionOnly,
    /// The write would mutate bus truth directly.
    TruthWrite,
}

/// The resolved authorization material for one privileged read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivilegedReadAuthorization {
    /// The privileged scope validated by the read-only policy.
    pub scope: PrivilegedAccessScope,
    /// The trusted authorization reference provided by the caller.
    pub authorization_ref: AuthorizationRef,
}

/// A guard that rejects truth writes from read-only output code paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadOnlyOutputPolicy {
    /// The stable policy reference.
    pub policy_ref: ReadOnlyPolicyRef,
}

impl ReadOnlyOutputPolicy {
    /// Returns the default read-only output guard for the bus.
    pub fn default_for_projection() -> Self {
        Self {
            policy_ref: ReadOnlyPolicyRef::new("policy.bus.read_only_projection"),
        }
    }

    /// Returns whether the requested write intent remains projection-only.
    pub fn allows_projection_write(&self, intent: ProjectionWriteIntent) -> bool {
        matches!(intent, ProjectionWriteIntent::ProjectionOnly)
    }

    /// Returns whether the requested intent attempts to mutate truth.
    pub fn rejects_truth_write(&self, intent: ProjectionWriteIntent) -> bool {
        matches!(intent, ProjectionWriteIntent::TruthWrite)
    }

    /// Validates the privileged seam for one sensitive read.
    pub fn authorize_sensitive_read(
        &self,
        scope: PrivilegedAccessScope,
        actor: &ActorContext,
        authorization_ref: Option<&AuthorizationRef>,
    ) -> Result<PrivilegedReadAuthorization, PrivilegedAccessRejectionReason> {
        let authorization_ref = authorization_ref
            .filter(|value| !value.as_str().trim().is_empty())
            .ok_or(PrivilegedAccessRejectionReason::MissingAuthorizationRef)?;
        if actor.role_refs.is_empty() {
            return Err(PrivilegedAccessRejectionReason::MissingRoleHint);
        }

        Ok(PrivilegedReadAuthorization {
            scope,
            authorization_ref: authorization_ref.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use bus_contracts::metadata::{
        ActorContext, ActorKind, ActorRef, AuditRef, AuthorizationRef, DeliveryMode, PayloadDigest,
        PayloadKind, PayloadRef, RequestOrigin, RoleRef, SourceRecordRef, SourceSystem,
        TargetScope, Timestamp,
    };

    use crate::audit::{
        AuditAction, BusAuditEntry, PrivilegedAccessRejectionReason, PrivilegedAccessScope,
        SubjectRef,
    };
    use crate::delivery::DeliveryRecord;
    use crate::publication::{PublicationMaterial, TransportSemantic};
    use crate::recovery::FailureMaterial;

    use super::{
        FailureSummaryProjection, PrivilegedReadAuthorization, ProjectionStatus,
        ProjectionWriteIntent, ReadOnlyOutputPolicy, TransportViewProjection,
    };

    fn actor() -> ActorContext {
        ActorContext::new(
            ActorRef::new("actor_projection", ActorKind::System),
            RequestOrigin::Job,
        )
    }

    fn privileged_actor() -> ActorContext {
        let mut actor = ActorContext::new(
            ActorRef::new("actor_projection_query", ActorKind::Human),
            RequestOrigin::Query,
        );
        actor.role_refs.push(RoleRef::new("role_projection_reader"));
        actor
    }

    fn audit(audit_ref: &str) -> BusAuditEntry {
        BusAuditEntry::record(
            AuditRef::new(audit_ref),
            SubjectRef::Delivery(bus_contracts::metadata::DeliveryId::new(
                "delivery_projection",
            )),
            AuditAction::DeliveryDelivered,
            actor(),
            bus_contracts::metadata::TraceContextRef::new("trace_projection"),
            Timestamp::new("2026-05-31T00:00:00Z"),
        )
    }

    fn delivery() -> DeliveryRecord {
        let material = PublicationMaterial {
            publication_id: bus_contracts::metadata::PublicationId::new("pub_projection"),
            source_system: SourceSystem::new("source_projection"),
            source_record_ref: SourceRecordRef::new("record_projection"),
            core_event_ref: bus_contracts::metadata::CoreEventRef::new("core_event_projection"),
            core_event_envelope_ref: None,
            payload_ref: PayloadRef::new("artifact_projection"),
            payload_kind: PayloadKind::ArtifactRef,
            payload_digest: PayloadDigest::new("sha256:projection"),
            delivery_mode: DeliveryMode::AtLeastOnce,
            target_scope: TargetScope {
                project_id: "project_projection".to_owned(),
                topic: "topic_projection".to_owned(),
            },
            outbox_fact_ref: None,
            actor: actor(),
            trace_ref: bus_contracts::metadata::TraceContextRef::new("trace_projection"),
        };
        let semantic = TransportSemantic::derive(
            material,
            bus_contracts::metadata::BackendCapabilityRef::from_profile(
                bus_contracts::metadata::BackendProfileRef::new("profile_projection"),
                bus_contracts::metadata::BackendKind::InMemory,
                bus_contracts::metadata::CapabilityVersion::new("v1"),
            ),
            bus_contracts::metadata::SubscriberScope {
                project_id: "project_projection".to_owned(),
                topic: "topic_projection".to_owned(),
            },
        )
        .expect("transport semantic should derive");

        DeliveryRecord::schedule(
            semantic,
            bus_contracts::metadata::SubscriberRef::new("subscriber_projection"),
            bus_contracts::metadata::IdempotencyKey::new("idem_projection"),
        )
        .expect("delivery should schedule")
    }

    #[test]
    fn transport_view_projection_can_be_marked_stale() {
        let mut projection = TransportViewProjection::derive(delivery(), audit("audit_projection"))
            .expect("projection should derive");

        projection
            .mark_stale(AuditRef::new("audit_projection_newer"))
            .expect("projection should become stale");

        assert_eq!(projection.status, ProjectionStatus::Stale);
        assert!(!projection.is_active());
    }

    #[test]
    fn failure_summary_projection_is_not_governance_decision() {
        let feedback = crate::feedback::FeedbackResult::fail(
            bus_contracts::metadata::DeliveryId::new("delivery_projection"),
            crate::feedback::FeedbackSource::new(
                bus_contracts::metadata::DeliveryAttemptId::new("attempt_projection"),
                bus_contracts::metadata::ExternalFeedbackRef::new("feedback_projection"),
            )
            .expect("feedback source should build"),
            bus_contracts::metadata::FeedbackReason::new("subscriber_failed"),
            actor(),
            Timestamp::new("2026-05-31T00:00:00Z"),
        )
        .expect("feedback should build");
        let history = crate::delivery::DeliveryHistoryEntry::transition(
            bus_contracts::metadata::DeliveryId::new("delivery_projection"),
            bus_contracts::metadata::DeliveryStatus::Delivered,
            bus_contracts::metadata::DeliveryStatus::Failed,
            bus_contracts::metadata::HistoryReason::feedback_fail(),
            Timestamp::new("2026-05-31T00:00:01Z"),
        );
        let material =
            FailureMaterial::from_feedback(feedback, history, AuditRef::new("audit_fail"))
                .expect("failure material should build");

        let projection =
            FailureSummaryProjection::derive(material, audit("audit_projection_failure"))
                .expect("projection should derive");

        assert!(!projection.is_governance_decision());
    }

    #[test]
    fn read_only_output_policy_rejects_truth_writes() {
        let policy = ReadOnlyOutputPolicy::default_for_projection();

        assert!(policy.allows_projection_write(ProjectionWriteIntent::ProjectionOnly));
        assert!(policy.rejects_truth_write(ProjectionWriteIntent::TruthWrite));
    }

    #[test]
    fn authorize_sensitive_read_requires_authorization_reference() {
        let policy = ReadOnlyOutputPolicy::default_for_projection();

        let error = policy
            .authorize_sensitive_read(
                PrivilegedAccessScope::FailureSummary,
                &privileged_actor(),
                None,
            )
            .expect_err("missing authorization reference must be rejected");

        assert_eq!(
            error,
            PrivilegedAccessRejectionReason::MissingAuthorizationRef
        );
    }

    #[test]
    fn authorize_sensitive_read_requires_role_hint() {
        let policy = ReadOnlyOutputPolicy::default_for_projection();

        let error = policy
            .authorize_sensitive_read(
                PrivilegedAccessScope::BusAuditTrail,
                &actor(),
                Some(&AuthorizationRef::new("auth_projection_query")),
            )
            .expect_err("missing role hint must be rejected");

        assert_eq!(error, PrivilegedAccessRejectionReason::MissingRoleHint);
    }

    #[test]
    fn authorize_sensitive_read_accepts_actor_and_authorization_reference() {
        let policy = ReadOnlyOutputPolicy::default_for_projection();

        let authorization = policy
            .authorize_sensitive_read(
                PrivilegedAccessScope::FailureSummary,
                &privileged_actor(),
                Some(&AuthorizationRef::new("auth_projection_query")),
            )
            .expect("privileged actor should be authorized");

        assert_eq!(
            authorization,
            PrivilegedReadAuthorization {
                scope: PrivilegedAccessScope::FailureSummary,
                authorization_ref: AuthorizationRef::new("auth_projection_query"),
            }
        );
    }
}

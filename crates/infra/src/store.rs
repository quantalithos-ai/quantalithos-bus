//! Shared in-memory state with staged transaction support.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use bus_application::{
    CommitReceipt, RepositoryError, UnitOfWorkError, UnitOfWorkHandle, UnitOfWorkPurpose,
};
use bus_contracts::metadata::{
    BackendId, DeadLetterId, DeliveryId, ExternalFeedbackRef, FailureMaterialId, FailureSummaryId,
    FeedbackId, IdempotencyKey, PageLimit, ProjectionVersion, PublicationId, ReplayApprovalRef,
    ReplayPreparationId, RetryPlanId, RetryScanCursor, Timestamp, TransportViewId, Version,
};
use bus_contracts::views::BackendHealthView;
use bus_domain::audit::{AuditChain, BusAuditEntry};
use bus_domain::delivery::DeliveryRecord;
use bus_domain::feedback::FeedbackResult;
use bus_domain::idempotency::{IdempotencyAnchor, IdempotencyConflict, IdempotencyScope};
use bus_domain::publication::{PublicationAcceptance, PublicationMaterial};
use bus_domain::read_output::{FailureSummaryProjection, TransportViewProjection};
use bus_domain::recovery::{DeadLetterEntry, FailureMaterial, ReplayPreparation, RetryPlan};

#[derive(Clone, Default)]
pub struct SharedMemoryStore {
    inner: Arc<Mutex<MemoryStoreInner>>,
}

#[derive(Default)]
struct MemoryStoreInner {
    next_transaction_id: u64,
    next_audit_sequence: u64,
    deliveries: BTreeMap<DeliveryId, DeliveryRecord>,
    delivery_versions: BTreeMap<DeliveryId, Version>,
    feedbacks: BTreeMap<FeedbackId, FeedbackResult>,
    feedback_versions: BTreeMap<FeedbackId, Version>,
    feedback_sources: HashMap<(DeliveryId, ExternalFeedbackRef), FeedbackId>,
    publications: BTreeMap<PublicationId, PublicationAcceptance>,
    publication_materials: BTreeMap<PublicationId, PublicationMaterial>,
    publication_versions: BTreeMap<PublicationId, Version>,
    transport_view_projections: BTreeMap<TransportViewId, TransportViewProjection>,
    failure_summary_projections: BTreeMap<FailureSummaryId, FailureSummaryProjection>,
    failure_summary_versions: BTreeMap<FailureSummaryId, ProjectionVersion>,
    backend_health_views: BTreeMap<BackendId, BackendHealthView>,
    backend_health_versions: BTreeMap<BackendId, ProjectionVersion>,
    retry_plans: BTreeMap<RetryPlanId, RetryPlan>,
    retry_plan_versions: BTreeMap<RetryPlanId, Version>,
    active_retry_plans: HashMap<DeliveryId, RetryPlanId>,
    dead_letters: BTreeMap<DeadLetterId, DeadLetterEntry>,
    dead_letter_versions: BTreeMap<DeadLetterId, Version>,
    active_dead_letters: HashMap<DeliveryId, DeadLetterId>,
    replay_preparations: BTreeMap<ReplayPreparationId, ReplayPreparation>,
    replay_preparation_versions: BTreeMap<ReplayPreparationId, Version>,
    replay_ready_by_dead_letter_approval:
        HashMap<(DeadLetterId, ReplayApprovalRef), ReplayPreparationId>,
    failure_materials: BTreeMap<FailureMaterialId, FailureMaterial>,
    failure_material_versions: BTreeMap<FailureMaterialId, Version>,
    anchors: HashMap<(IdempotencyScope, IdempotencyKey), IdempotencyAnchor>,
    conflicts: Vec<IdempotencyConflict>,
    audits: Vec<BusAuditEntry>,
    transactions: HashMap<u64, StagedTransaction>,
}

#[derive(Default)]
struct StagedTransaction {
    _purpose: Option<UnitOfWorkPurpose>,
    deliveries: BTreeMap<DeliveryId, DeliveryRecord>,
    feedbacks: BTreeMap<FeedbackId, FeedbackResult>,
    feedback_sources: HashMap<(DeliveryId, ExternalFeedbackRef), FeedbackId>,
    publications: BTreeMap<PublicationId, PublicationAcceptance>,
    publication_materials: BTreeMap<PublicationId, PublicationMaterial>,
    transport_view_projections: BTreeMap<TransportViewId, TransportViewProjection>,
    failure_summary_projections: BTreeMap<FailureSummaryId, FailureSummaryProjection>,
    backend_health_views: BTreeMap<BackendId, BackendHealthView>,
    retry_plans: BTreeMap<RetryPlanId, RetryPlan>,
    dead_letters: BTreeMap<DeadLetterId, DeadLetterEntry>,
    replay_preparations: BTreeMap<ReplayPreparationId, ReplayPreparation>,
    failure_materials: BTreeMap<FailureMaterialId, FailureMaterial>,
    anchors: HashMap<(IdempotencyScope, IdempotencyKey), IdempotencyAnchor>,
    conflicts: Vec<IdempotencyConflict>,
    audits: Vec<BusAuditEntry>,
}

impl SharedMemoryStore {
    /// Creates a fresh shared memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Begins a staged write transaction.
    pub fn begin_transaction(
        &self,
        purpose: UnitOfWorkPurpose,
    ) -> Result<UnitOfWorkHandle, UnitOfWorkError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        inner.next_transaction_id += 1;
        let transaction_id = inner.next_transaction_id;
        inner.transactions.insert(
            transaction_id,
            StagedTransaction {
                _purpose: Some(purpose),
                ..StagedTransaction::default()
            },
        );

        Ok(UnitOfWorkHandle {
            transaction_id,
            purpose,
        })
    }

    /// Commits a staged transaction.
    pub fn commit_transaction(
        &self,
        handle: UnitOfWorkHandle,
    ) -> Result<CommitReceipt, UnitOfWorkError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let staged = inner
            .transactions
            .remove(&handle.transaction_id)
            .ok_or(UnitOfWorkError::InvalidHandle)?;

        for (delivery_id, delivery) in staged.deliveries {
            inner
                .deliveries
                .insert(delivery_id.clone(), delivery.clone());
            inner
                .delivery_versions
                .insert(delivery_id, delivery.version());
        }
        for (feedback_id, feedback) in staged.feedbacks {
            inner
                .feedbacks
                .insert(feedback_id.clone(), feedback.clone());
            inner
                .feedback_versions
                .insert(feedback_id, feedback.version());
        }
        for (key, feedback_id) in staged.feedback_sources {
            inner.feedback_sources.insert(key, feedback_id);
        }
        for (publication_id, acceptance) in staged.publications {
            let version = inner
                .publication_versions
                .get(&publication_id)
                .copied()
                .unwrap_or(0);
            inner
                .publications
                .insert(publication_id.clone(), acceptance);
            inner
                .publication_versions
                .insert(publication_id, version + 1);
        }
        for (publication_id, material) in staged.publication_materials {
            inner.publication_materials.insert(publication_id, material);
        }
        for (view_id, projection) in staged.transport_view_projections {
            inner.transport_view_projections.insert(view_id, projection);
        }
        for (summary_id, projection) in staged.failure_summary_projections {
            let next_version =
                ProjectionVersion::next_after(inner.failure_summary_versions.get(&summary_id));
            inner
                .failure_summary_versions
                .insert(summary_id.clone(), next_version);
            inner
                .failure_summary_projections
                .insert(summary_id, projection);
        }
        for (backend_id, view) in staged.backend_health_views {
            let next_version =
                ProjectionVersion::next_after(inner.backend_health_versions.get(&backend_id));
            inner
                .backend_health_versions
                .insert(backend_id.clone(), next_version);
            inner.backend_health_views.insert(backend_id, view);
        }
        for (retry_plan_id, retry_plan) in staged.retry_plans {
            if retry_plan.status == bus_contracts::metadata::RetryPlanStatus::Scheduled {
                inner
                    .active_retry_plans
                    .insert(retry_plan.delivery_id.clone(), retry_plan_id.clone());
            } else if inner.active_retry_plans.get(&retry_plan.delivery_id) == Some(&retry_plan_id)
            {
                inner.active_retry_plans.remove(&retry_plan.delivery_id);
            }
            inner
                .retry_plan_versions
                .insert(retry_plan_id.clone(), retry_plan.version());
            inner.retry_plans.insert(retry_plan_id, retry_plan);
        }
        for (dead_letter_id, dead_letter) in staged.dead_letters {
            if dead_letter.status != bus_contracts::metadata::DeadLetterStatus::Closed {
                inner
                    .active_dead_letters
                    .insert(dead_letter.delivery_id.clone(), dead_letter_id.clone());
            } else if inner.active_dead_letters.get(&dead_letter.delivery_id)
                == Some(&dead_letter_id)
            {
                inner.active_dead_letters.remove(&dead_letter.delivery_id);
            }
            inner
                .dead_letter_versions
                .insert(dead_letter_id.clone(), dead_letter.version());
            inner.dead_letters.insert(dead_letter_id, dead_letter);
        }
        for (replay_id, preparation) in staged.replay_preparations {
            if let Some(approval_ref) = preparation.approval_ref.clone() {
                inner.replay_ready_by_dead_letter_approval.insert(
                    (preparation.dead_letter_id.clone(), approval_ref),
                    replay_id.clone(),
                );
            }
            inner
                .replay_preparation_versions
                .insert(replay_id.clone(), preparation.version());
            inner.replay_preparations.insert(replay_id, preparation);
        }
        for (failure_material_id, material) in staged.failure_materials {
            inner
                .failure_material_versions
                .insert(failure_material_id.clone(), material.version());
            inner
                .failure_materials
                .insert(failure_material_id, material);
        }
        for (key, anchor) in staged.anchors {
            inner.anchors.insert(key, anchor);
        }
        inner.conflicts.extend(staged.conflicts);
        for audit in staged.audits {
            inner.next_audit_sequence += 1;
            inner.audits.push(audit);
        }

        Ok(CommitReceipt {
            transaction_id: handle.transaction_id,
        })
    }

    /// Rolls back a staged transaction.
    pub fn rollback_transaction(&self, handle: UnitOfWorkHandle) -> Result<(), UnitOfWorkError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        inner
            .transactions
            .remove(&handle.transaction_id)
            .map(|_| ())
            .ok_or(UnitOfWorkError::InvalidHandle)
    }

    /// Stages a publication acceptance insert.
    pub fn stage_publication(
        &self,
        transaction_id: u64,
        acceptance: PublicationAcceptance,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner.publications.contains_key(&acceptance.publication_id) {
            return Err(RepositoryError::UniqueViolation);
        }
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if staged.publications.contains_key(&acceptance.publication_id) {
            return Err(RepositoryError::UniqueViolation);
        }

        staged
            .publications
            .insert(acceptance.publication_id.clone(), acceptance.clone());
        Ok(inner
            .publication_versions
            .get(&acceptance.publication_id)
            .copied()
            .unwrap_or(0)
            + 1)
    }

    /// Stages a publication-material snapshot.
    pub fn stage_publication_material(
        &self,
        transaction_id: u64,
        material: PublicationMaterial,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner
            .publication_materials
            .contains_key(&material.publication_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if staged
            .publication_materials
            .contains_key(&material.publication_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }
        staged
            .publication_materials
            .insert(material.publication_id.clone(), material);
        Ok(())
    }

    /// Returns a committed publication acceptance.
    pub fn publication(&self, publication_id: &PublicationId) -> Option<PublicationAcceptance> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .publications
            .get(publication_id)
            .cloned()
    }

    /// Returns a committed publication-material snapshot.
    pub fn publication_material(
        &self,
        publication_id: &PublicationId,
    ) -> Option<PublicationMaterial> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .publication_materials
            .get(publication_id)
            .cloned()
    }

    /// Seeds a committed retry plan for tests and fake workers.
    pub fn seed_retry_plan(&self, retry_plan: RetryPlan) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner.retry_plans.contains_key(&retry_plan.retry_plan_id) {
            return Err(RepositoryError::UniqueViolation);
        }
        if retry_plan.status == bus_contracts::metadata::RetryPlanStatus::Scheduled
            && inner
                .active_retry_plans
                .contains_key(&retry_plan.delivery_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }

        let mut committed = retry_plan;
        committed.set_version(1);
        if committed.status == bus_contracts::metadata::RetryPlanStatus::Scheduled {
            inner.active_retry_plans.insert(
                committed.delivery_id.clone(),
                committed.retry_plan_id.clone(),
            );
        }
        inner
            .retry_plan_versions
            .insert(committed.retry_plan_id.clone(), committed.version());
        inner
            .retry_plans
            .insert(committed.retry_plan_id.clone(), committed);
        Ok(1)
    }

    /// Stages a retry-plan create or update.
    pub fn stage_retry_plan_save(
        &self,
        transaction_id: u64,
        retry_plan: RetryPlan,
        expected_version: Option<Version>,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let committed_version = inner
            .retry_plan_versions
            .get(&retry_plan.retry_plan_id)
            .copied()
            .unwrap_or(0);
        match expected_version {
            Some(version) if version != committed_version => {
                return Err(RepositoryError::VersionConflict);
            }
            None if committed_version != 0 => return Err(RepositoryError::UniqueViolation),
            _ => {}
        }

        if retry_plan.status == bus_contracts::metadata::RetryPlanStatus::Scheduled {
            if let Some(existing_id) = inner.active_retry_plans.get(&retry_plan.delivery_id) {
                if existing_id != &retry_plan.retry_plan_id {
                    return Err(RepositoryError::UniqueViolation);
                }
            }
        }

        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if retry_plan.status == bus_contracts::metadata::RetryPlanStatus::Scheduled
            && staged.retry_plans.values().any(|candidate| {
                candidate.delivery_id == retry_plan.delivery_id
                    && candidate.status == bus_contracts::metadata::RetryPlanStatus::Scheduled
                    && candidate.retry_plan_id != retry_plan.retry_plan_id
            })
        {
            return Err(RepositoryError::UniqueViolation);
        }

        let mut staged_retry_plan = retry_plan;
        staged_retry_plan.set_version(committed_version + 1);
        let new_version = staged_retry_plan.version();
        staged
            .retry_plans
            .insert(staged_retry_plan.retry_plan_id.clone(), staged_retry_plan);

        Ok(new_version)
    }

    /// Returns one committed retry plan.
    pub fn retry_plan(&self, retry_plan_id: &RetryPlanId) -> Option<RetryPlan> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .retry_plans
            .get(retry_plan_id)
            .cloned()
    }

    /// Returns due retry plans ordered by identifier.
    pub fn due_retry_plans(
        &self,
        cursor: &RetryScanCursor,
        limit: PageLimit,
        now: &Timestamp,
    ) -> Vec<RetryPlan> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .retry_plans
            .values()
            .filter(|plan| {
                plan.status == bus_contracts::metadata::RetryPlanStatus::Scheduled
                    && plan.next_attempt_at.as_str() <= now.as_str()
                    && plan.retry_plan_id.as_str() > cursor.as_str()
            })
            .take(limit.get() as usize)
            .cloned()
            .collect()
    }

    /// Seeds committed failure material for tests.
    pub fn seed_failure_material(
        &self,
        material: FailureMaterial,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner
            .failure_materials
            .contains_key(&material.failure_material_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }

        let mut committed = material;
        committed.set_version(1);
        inner
            .failure_material_versions
            .insert(committed.failure_material_id.clone(), committed.version());
        inner
            .failure_materials
            .insert(committed.failure_material_id.clone(), committed);
        Ok(1)
    }

    /// Returns one committed failure material.
    pub fn failure_material(
        &self,
        failure_material_id: &FailureMaterialId,
    ) -> Option<FailureMaterial> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .failure_materials
            .get(failure_material_id)
            .cloned()
    }

    /// Seeds a committed dead-letter entry for tests.
    pub fn seed_dead_letter(&self, entry: DeadLetterEntry) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner.dead_letters.contains_key(&entry.dead_letter_id) {
            return Err(RepositoryError::UniqueViolation);
        }
        if entry.status != bus_contracts::metadata::DeadLetterStatus::Closed
            && inner.active_dead_letters.contains_key(&entry.delivery_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }

        let mut committed = entry;
        committed.set_version(1);
        if committed.status != bus_contracts::metadata::DeadLetterStatus::Closed {
            inner.active_dead_letters.insert(
                committed.delivery_id.clone(),
                committed.dead_letter_id.clone(),
            );
        }
        inner
            .dead_letter_versions
            .insert(committed.dead_letter_id.clone(), committed.version());
        inner
            .dead_letters
            .insert(committed.dead_letter_id.clone(), committed);
        Ok(1)
    }

    /// Stages one dead-letter save and links the existing failure material.
    pub fn stage_dead_letter_save(
        &self,
        transaction_id: u64,
        entry: DeadLetterEntry,
        material: FailureMaterial,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner.dead_letters.contains_key(&entry.dead_letter_id)
            || inner.active_dead_letters.contains_key(&entry.delivery_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }
        let current_material_version = inner
            .failure_material_versions
            .get(&material.failure_material_id)
            .copied()
            .ok_or(RepositoryError::VersionConflict)?;
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if staged.dead_letters.contains_key(&entry.dead_letter_id)
            || staged
                .dead_letters
                .values()
                .any(|candidate| candidate.delivery_id == entry.delivery_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }

        let mut staged_entry = entry;
        staged_entry.set_version(1);
        let mut staged_material = material;
        staged_material.dead_letter_ref = Some(bus_contracts::metadata::DeadLetterRef::from(
            staged_entry.dead_letter_id.clone(),
        ));
        staged_material.set_version(current_material_version + 1);
        let new_version = staged_entry.version();
        staged
            .dead_letters
            .insert(staged_entry.dead_letter_id.clone(), staged_entry);
        staged
            .failure_materials
            .insert(staged_material.failure_material_id.clone(), staged_material);

        Ok(new_version)
    }

    /// Returns one committed dead-letter entry.
    pub fn dead_letter(&self, dead_letter_id: &DeadLetterId) -> Option<DeadLetterEntry> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .dead_letters
            .get(dead_letter_id)
            .cloned()
    }

    /// Stages one replay preparation save.
    pub fn stage_replay_preparation_save(
        &self,
        transaction_id: u64,
        preparation: ReplayPreparation,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner
            .replay_preparations
            .contains_key(&preparation.replay_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }
        if let Some(approval_ref) = preparation.approval_ref.clone() {
            if inner
                .replay_ready_by_dead_letter_approval
                .contains_key(&(preparation.dead_letter_id.clone(), approval_ref.clone()))
            {
                return Err(RepositoryError::UniqueViolation);
            }
        }
        let current_version = inner
            .replay_preparation_versions
            .get(&preparation.replay_id)
            .copied()
            .unwrap_or(0);
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if staged
            .replay_preparations
            .contains_key(&preparation.replay_id)
        {
            return Err(RepositoryError::UniqueViolation);
        }
        if let Some(approval_ref) = preparation.approval_ref.clone() {
            if staged.replay_preparations.values().any(|candidate| {
                candidate.dead_letter_id == preparation.dead_letter_id
                    && candidate.approval_ref == Some(approval_ref.clone())
            }) {
                return Err(RepositoryError::UniqueViolation);
            }
        }

        let mut staged_preparation = preparation;
        staged_preparation.set_version(current_version + 1);
        let new_version = staged_preparation.version();
        staged
            .replay_preparations
            .insert(staged_preparation.replay_id.clone(), staged_preparation);

        Ok(new_version)
    }

    /// Returns one committed replay preparation.
    pub fn replay_preparation(
        &self,
        replay_preparation_id: &ReplayPreparationId,
    ) -> Option<ReplayPreparation> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .replay_preparations
            .get(replay_preparation_id)
            .cloned()
    }

    /// Stages a new idempotency anchor.
    pub fn stage_idempotency_anchor(
        &self,
        transaction_id: u64,
        anchor: IdempotencyAnchor,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let key = (anchor.scope.clone(), anchor.key.clone());
        if inner.anchors.contains_key(&key) {
            return Err(RepositoryError::UniqueViolation);
        }
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if staged.anchors.contains_key(&key) {
            return Err(RepositoryError::UniqueViolation);
        }
        staged.anchors.insert(key, anchor);
        Ok(())
    }

    /// Returns a committed idempotency anchor.
    pub fn idempotency_anchor(
        &self,
        scope: &IdempotencyScope,
        key: &IdempotencyKey,
    ) -> Option<IdempotencyAnchor> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .anchors
            .get(&(scope.clone(), key.clone()))
            .cloned()
    }

    /// Stages an idempotency conflict summary.
    pub fn stage_idempotency_conflict(
        &self,
        transaction_id: u64,
        _scope: IdempotencyScope,
        _key: IdempotencyKey,
        conflict: IdempotencyConflict,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        staged.conflicts.push(conflict);
        Ok(())
    }

    /// Returns committed conflict summaries.
    pub fn idempotency_conflicts(&self) -> Vec<IdempotencyConflict> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .conflicts
            .clone()
    }

    /// Stages an audit entry.
    pub fn stage_audit_entry(
        &self,
        transaction_id: u64,
        entry: BusAuditEntry,
    ) -> Result<u64, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let next_sequence = inner.next_audit_sequence + 1;
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        staged.audits.push(entry);
        Ok(next_sequence)
    }

    /// Returns committed audit entries.
    pub fn audit_entries(&self) -> Vec<BusAuditEntry> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .audits
            .clone()
    }

    /// Loads one committed audit chain by reference.
    pub fn audit_chain(&self, chain_ref: &bus_contracts::metadata::AuditChainRef) -> AuditChain {
        let entries = self
            .inner
            .lock()
            .expect("memory store lock poisoned")
            .audits
            .iter()
            .filter(|entry| {
                bus_contracts::metadata::AuditChainRef::from_audit_ref(&entry.audit_ref)
                    == *chain_ref
            })
            .cloned()
            .collect();

        AuditChain {
            chain_ref: chain_ref.clone(),
            entries,
        }
    }

    /// Stages a new feedback result insert.
    pub fn stage_feedback(
        &self,
        transaction_id: u64,
        feedback: FeedbackResult,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner.feedbacks.contains_key(&feedback.feedback_id) {
            return Err(RepositoryError::UniqueViolation);
        }
        let source_key = (
            feedback.delivery_id.clone(),
            feedback.source.external_feedback_ref.clone(),
        );
        if inner.feedback_sources.contains_key(&source_key) {
            return Err(RepositoryError::UniqueViolation);
        }
        let current_version = inner
            .feedback_versions
            .get(&feedback.feedback_id)
            .copied()
            .unwrap_or(0);
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if staged.feedbacks.contains_key(&feedback.feedback_id)
            || staged.feedback_sources.contains_key(&source_key)
        {
            return Err(RepositoryError::UniqueViolation);
        }

        let mut staged_feedback = feedback;
        staged_feedback.set_version(current_version + 1);
        staged
            .feedback_sources
            .insert(source_key, staged_feedback.feedback_id.clone());
        staged
            .feedbacks
            .insert(staged_feedback.feedback_id.clone(), staged_feedback);

        Ok(current_version + 1)
    }

    /// Returns a committed feedback result.
    pub fn feedback(&self, feedback_id: &FeedbackId) -> Option<FeedbackResult> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .feedbacks
            .get(feedback_id)
            .cloned()
    }

    /// Returns all committed feedback results for tests.
    pub fn feedbacks(&self) -> Vec<FeedbackResult> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .feedbacks
            .values()
            .cloned()
            .collect()
    }

    /// Returns committed feedback results for one delivery ordered by observed time.
    pub fn feedbacks_by_delivery(&self, delivery_id: &DeliveryId) -> Vec<FeedbackResult> {
        let mut feedbacks = self
            .inner
            .lock()
            .expect("memory store lock poisoned")
            .feedbacks
            .values()
            .filter(|feedback| &feedback.delivery_id == delivery_id)
            .cloned()
            .collect::<Vec<_>>();
        feedbacks.sort_by(|left, right| left.observed_at.as_str().cmp(right.observed_at.as_str()));
        feedbacks
    }

    /// Seeds a committed delivery aggregate for tests and fake workers.
    pub fn seed_delivery(&self, delivery: DeliveryRecord) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner.deliveries.contains_key(&delivery.delivery_id) {
            return Err(RepositoryError::UniqueViolation);
        }

        let mut committed = delivery;
        committed.set_version(1);
        inner
            .delivery_versions
            .insert(committed.delivery_id.clone(), committed.version());
        inner
            .deliveries
            .insert(committed.delivery_id.clone(), committed);
        Ok(1)
    }

    /// Stages a delivery aggregate update.
    pub fn stage_delivery_save(
        &self,
        transaction_id: u64,
        delivery: DeliveryRecord,
        expected_version: Version,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let committed_version = inner
            .delivery_versions
            .get(&delivery.delivery_id)
            .copied()
            .ok_or(RepositoryError::VersionConflict)?;
        if committed_version != expected_version {
            return Err(RepositoryError::VersionConflict);
        }
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        let mut staged_delivery = delivery;
        staged_delivery.set_version(expected_version + 1);
        let new_version = staged_delivery.version();
        staged
            .deliveries
            .insert(staged_delivery.delivery_id.clone(), staged_delivery);

        Ok(new_version)
    }

    /// Returns a committed delivery aggregate.
    pub fn delivery(&self, delivery_id: &DeliveryId) -> Option<DeliveryRecord> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .deliveries
            .get(delivery_id)
            .cloned()
    }

    /// Returns committed schedulable deliveries ordered by identifier.
    pub fn schedulable_deliveries(&self, cursor: &str, limit: u32) -> Vec<DeliveryRecord> {
        let inner = self.inner.lock().expect("memory store lock poisoned");
        let has_after_cursor = inner
            .deliveries
            .keys()
            .any(|delivery_id| delivery_id.as_str() > cursor);

        inner
            .deliveries
            .values()
            .filter(|delivery| {
                delivery.status == bus_contracts::metadata::DeliveryStatus::Scheduled
                    && (!has_after_cursor || delivery.delivery_id.as_str() > cursor)
            })
            .take(limit as usize)
            .cloned()
            .collect()
    }

    /// Stages one transport-view projection upsert.
    pub fn stage_transport_view_projection(
        &self,
        transaction_id: u64,
        projection: TransportViewProjection,
    ) -> Result<ProjectionVersion, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let current = inner
            .transport_view_projections
            .get(&projection.view_id)
            .map(|value| value.version.clone());
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        let mut staged_projection = projection;
        staged_projection.version = ProjectionVersion::next_after(current.as_ref());
        let version = staged_projection.version.clone();
        staged
            .transport_view_projections
            .insert(staged_projection.view_id.clone(), staged_projection);
        Ok(version)
    }

    /// Returns one committed transport-view projection.
    pub fn transport_view_projection(
        &self,
        transport_view_id: &TransportViewId,
    ) -> Option<TransportViewProjection> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .transport_view_projections
            .get(transport_view_id)
            .cloned()
    }

    /// Seeds one committed transport-view projection for tests.
    pub fn seed_transport_view_projection(
        &self,
        projection: TransportViewProjection,
    ) -> Result<ProjectionVersion, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let mut committed = projection;
        committed.version = ProjectionVersion::next_after(
            inner
                .transport_view_projections
                .get(&committed.view_id)
                .map(|value| &value.version),
        );
        let version = committed.version.clone();
        inner
            .transport_view_projections
            .insert(committed.view_id.clone(), committed);
        Ok(version)
    }

    /// Stages one failure-summary projection upsert.
    pub fn stage_failure_summary_projection(
        &self,
        transaction_id: u64,
        projection: FailureSummaryProjection,
    ) -> Result<ProjectionVersion, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let version = ProjectionVersion::next_after(
            inner.failure_summary_versions.get(&projection.summary_id),
        );
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        staged
            .failure_summary_projections
            .insert(projection.summary_id.clone(), projection);
        Ok(version)
    }

    /// Returns one committed failure-summary projection.
    pub fn failure_summary_projection(
        &self,
        failure_summary_id: &FailureSummaryId,
    ) -> Option<FailureSummaryProjection> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .failure_summary_projections
            .get(failure_summary_id)
            .cloned()
    }

    /// Seeds one committed failure-summary projection for tests.
    pub fn seed_failure_summary_projection(
        &self,
        projection: FailureSummaryProjection,
    ) -> Result<ProjectionVersion, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let version = ProjectionVersion::next_after(
            inner.failure_summary_versions.get(&projection.summary_id),
        );
        inner
            .failure_summary_versions
            .insert(projection.summary_id.clone(), version.clone());
        inner
            .failure_summary_projections
            .insert(projection.summary_id.clone(), projection);
        Ok(version)
    }

    /// Stages one backend-health view upsert.
    pub fn stage_backend_health_view(
        &self,
        transaction_id: u64,
        view: BackendHealthView,
    ) -> Result<ProjectionVersion, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let version =
            ProjectionVersion::next_after(inner.backend_health_versions.get(&view.backend_id));
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        staged
            .backend_health_views
            .insert(view.backend_id.clone(), view);
        Ok(version)
    }

    /// Returns one committed backend-health view.
    pub fn backend_health_view(&self, backend_id: &BackendId) -> Option<BackendHealthView> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .backend_health_views
            .get(backend_id)
            .cloned()
    }

    /// Seeds one committed backend-health view for tests.
    pub fn seed_backend_health_view(
        &self,
        view: BackendHealthView,
    ) -> Result<ProjectionVersion, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let version =
            ProjectionVersion::next_after(inner.backend_health_versions.get(&view.backend_id));
        inner
            .backend_health_versions
            .insert(view.backend_id.clone(), version.clone());
        inner
            .backend_health_views
            .insert(view.backend_id.clone(), view);
        Ok(version)
    }
}

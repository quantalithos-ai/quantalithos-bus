//! Outbox relay job service.

use bus_contracts::events::CommittedOutboxFactInput;
use bus_contracts::jobs::{OutboxRelayJobResult, RunOutboxRelayJob};
use bus_contracts::metadata::{
    ActorContext, ConsumerMarker, EventMetadata, JobMetadata, PageLimit,
};

use crate::errors::ApplicationError;
use crate::ports::OutboxFactSourcePort;
use crate::services::publication::OutboxPublicationAcceptanceUseCase;

/// The outbox-relay job use-case contract.
pub trait OutboxRelayUseCase: Send + Sync {
    /// Runs one committed-outbox relay batch.
    async fn run(
        &self,
        job: RunOutboxRelayJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<OutboxRelayJobResult, ApplicationError>;
}

/// Dependencies for the outbox-relay service.
pub struct OutboxRelayServiceDeps<S, P> {
    /// Source port for committed outbox facts.
    pub outbox_source: S,
    /// Publication acceptance use case reused for each fact.
    pub publication_service: P,
}

/// Application service for `RunOutboxRelay`.
pub struct OutboxRelayService<S, P> {
    deps: OutboxRelayServiceDeps<S, P>,
}

impl<S, P> OutboxRelayService<S, P> {
    /// Creates a new outbox-relay service.
    pub fn new(deps: OutboxRelayServiceDeps<S, P>) -> Self {
        Self { deps }
    }
}

impl<S, P> OutboxRelayUseCase for OutboxRelayService<S, P>
where
    S: OutboxFactSourcePort,
    P: OutboxPublicationAcceptanceUseCase,
{
    async fn run(
        &self,
        job: RunOutboxRelayJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<OutboxRelayJobResult, ApplicationError> {
        let _ = job.dry_run;
        let page = self
            .deps
            .outbox_source
            .poll_committed(job.cursor.clone(), PageLimit::new(job.batch_size))
            .await?;
        let mut result = OutboxRelayJobResult::start(meta.job_run_id.clone(), page.next_cursor);
        let mut cursor_must_replay = false;

        for fact in page.items {
            let fact_ref = fact.committed_fact_ref.clone();
            let input = CommittedOutboxFactInput::from_fact(fact);
            let consume_result = self
                .deps
                .publication_service
                .accept_from_outbox_fact(
                    input,
                    actor.clone(),
                    EventMetadata::from_job(meta.clone()),
                )
                .await;

            match consume_result {
                Ok(_) => match self
                    .deps
                    .outbox_source
                    .ack_consumed(fact_ref, ConsumerMarker::bus())
                    .await
                {
                    Ok(()) => result.record_accepted(),
                    Err(_) => {
                        result.record_failed();
                        cursor_must_replay = true;
                    }
                },
                Err(error) if error.is_rejected_item() => match self
                    .deps
                    .outbox_source
                    .ack_consumed(fact_ref, ConsumerMarker::bus())
                    .await
                {
                    Ok(()) => result.record_rejected(),
                    Err(_) => {
                        result.record_failed();
                        cursor_must_replay = true;
                    }
                },
                Err(_) => {
                    result.record_failed();
                    cursor_must_replay = true;
                }
            }
        }

        if cursor_must_replay {
            result.set_next_cursor(job.cursor);
        }

        Ok(result)
    }
}

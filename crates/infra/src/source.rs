//! In-memory committed outbox source fixture and adapter.

use std::sync::{Arc, Mutex};

use bus_application::{OutboxFactSourcePort, SourcePortError};
use bus_contracts::events::{CommittedOutboxFact, CommittedOutboxFactPage};
use bus_contracts::metadata::{CommittedOutboxFactRef, ConsumerMarker, OutboxCursor, PageLimit};

/// Fixture-level failure for seeding duplicate committed facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutboxSourceFixtureError {
    /// The committed fact reference is already present in the source fixture.
    DuplicateFactRef(CommittedOutboxFactRef),
}

#[derive(Clone, Debug)]
struct StoredCommittedOutboxFact {
    cursor: OutboxCursor,
    fact: CommittedOutboxFact,
    acknowledged_marker: Option<ConsumerMarker>,
}

#[derive(Default)]
struct OutboxSourceInner {
    facts: Vec<StoredCommittedOutboxFact>,
    fail_next_poll: Option<SourcePortError>,
    fail_next_ack: Option<SourcePortError>,
}

/// Shared source state for the in-memory committed-fact adapter.
#[derive(Clone, Default)]
pub struct SharedOutboxSource {
    inner: Arc<Mutex<OutboxSourceInner>>,
}

impl SharedOutboxSource {
    /// Creates a fresh shared committed-fact source fixture.
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds one committed fact into the source fixture.
    pub fn seed_committed(
        &self,
        fact: CommittedOutboxFact,
    ) -> Result<OutboxCursor, OutboxSourceFixtureError> {
        let mut inner = self.inner.lock().expect("outbox source lock poisoned");
        if inner
            .facts
            .iter()
            .any(|stored| stored.fact.committed_fact_ref == fact.committed_fact_ref)
        {
            return Err(OutboxSourceFixtureError::DuplicateFactRef(
                fact.committed_fact_ref,
            ));
        }

        let cursor = OutboxCursor::new(format!(
            "outbox_cursor_{}",
            fact.committed_fact_ref.as_str()
        ));
        inner.facts.push(StoredCommittedOutboxFact {
            cursor: cursor.clone(),
            fact,
            acknowledged_marker: None,
        });

        Ok(cursor)
    }

    /// Injects the next poll failure.
    pub fn fail_next_poll(&self, error: SourcePortError) {
        self.inner
            .lock()
            .expect("outbox source lock poisoned")
            .fail_next_poll = Some(error);
    }

    /// Injects the next acknowledgement failure.
    pub fn fail_next_ack(&self, error: SourcePortError) {
        self.inner
            .lock()
            .expect("outbox source lock poisoned")
            .fail_next_ack = Some(error);
    }

    /// Returns the persisted acknowledgement marker for a committed fact.
    pub fn acknowledged_marker(&self, fact_ref: &CommittedOutboxFactRef) -> Option<ConsumerMarker> {
        self.inner
            .lock()
            .expect("outbox source lock poisoned")
            .facts
            .iter()
            .find(|stored| &stored.fact.committed_fact_ref == fact_ref)
            .and_then(|stored| stored.acknowledged_marker.clone())
    }
}

/// In-memory adapter that exposes the committed outbox source port.
#[derive(Clone)]
pub struct InMemoryOutboxFactSourceAdapter {
    source: SharedOutboxSource,
}

impl InMemoryOutboxFactSourceAdapter {
    /// Creates a new source adapter over shared outbox fixture state.
    pub fn new(source: SharedOutboxSource) -> Self {
        Self { source }
    }
}

impl OutboxFactSourcePort for InMemoryOutboxFactSourceAdapter {
    async fn poll_committed(
        &self,
        cursor: OutboxCursor,
        limit: PageLimit,
    ) -> Result<CommittedOutboxFactPage, SourcePortError> {
        let mut inner = self
            .source
            .inner
            .lock()
            .expect("outbox source lock poisoned");
        if let Some(error) = inner.fail_next_poll.take() {
            return Err(error);
        }

        if limit.get() == 0 {
            return Ok(CommittedOutboxFactPage::empty(cursor));
        }

        let start_index = if cursor == OutboxCursor::origin() {
            0
        } else {
            inner
                .facts
                .iter()
                .position(|stored| stored.cursor == cursor)
                .map(|index| index + 1)
                .ok_or(SourcePortError::CursorInvalid)?
        };

        let mut items = Vec::new();
        let mut next_cursor = cursor.clone();

        for stored in inner.facts.iter().skip(start_index) {
            next_cursor = stored.cursor.clone();
            if stored.acknowledged_marker.is_none() {
                items.push(stored.fact.clone());
                if items.len() == limit.get() as usize {
                    break;
                }
            }
        }

        Ok(CommittedOutboxFactPage { items, next_cursor })
    }

    async fn ack_consumed(
        &self,
        fact_ref: CommittedOutboxFactRef,
        marker: ConsumerMarker,
    ) -> Result<(), SourcePortError> {
        let mut inner = self
            .source
            .inner
            .lock()
            .expect("outbox source lock poisoned");
        if let Some(error) = inner.fail_next_ack.take() {
            return Err(error);
        }

        let stored = inner
            .facts
            .iter_mut()
            .find(|stored| stored.fact.committed_fact_ref == fact_ref)
            .ok_or(SourcePortError::AckFailed)?;
        stored.acknowledged_marker = Some(marker);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use bus_contracts::fixtures::{OutboxFixtureBuilder, TestRunBuilder};

    use super::*;

    fn noop_raw_waker() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}

        RawWaker::new(
            std::ptr::null(),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop),
        )
    }

    fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
        let mut context = Context::from_waker(&waker);
        let mut future = pin!(future);

        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn outbox_source_polls_committed_facts_with_cursor_progression() {
        let run = TestRunBuilder::new("obx-source-001").build();
        let builder = OutboxFixtureBuilder::new(run);
        let source = SharedOutboxSource::new();
        let adapter = InMemoryOutboxFactSourceAdapter::new(source.clone());
        let first = builder.committed_fact();
        let second = builder.second_committed_fact();
        let first_cursor = source
            .seed_committed(first.clone())
            .expect("first fact should seed");
        source
            .seed_committed(second.clone())
            .expect("second fact should seed");

        let first_page =
            block_on(adapter.poll_committed(builder.origin_cursor(), PageLimit::new(1)))
                .expect("first page should load");
        let second_page = block_on(adapter.poll_committed(first_cursor, PageLimit::new(1)))
            .expect("second page should load");

        assert_eq!(first_page.items, vec![first]);
        assert_eq!(second_page.items, vec![second]);
    }

    #[test]
    fn outbox_source_rejects_unknown_cursor() {
        let source = SharedOutboxSource::new();
        let adapter = InMemoryOutboxFactSourceAdapter::new(source);

        let error = block_on(adapter.poll_committed(
            OutboxCursor::new("outbox_cursor_missing"),
            PageLimit::new(1),
        ))
        .expect_err("unknown cursor should fail");

        assert_eq!(error, SourcePortError::CursorInvalid);
    }

    #[test]
    fn outbox_source_acknowledges_fact_and_skips_it_on_later_polls() {
        let run = TestRunBuilder::new("obx-source-002").build();
        let builder = OutboxFixtureBuilder::new(run);
        let source = SharedOutboxSource::new();
        let adapter = InMemoryOutboxFactSourceAdapter::new(source.clone());
        let fact = builder.committed_fact();

        source
            .seed_committed(fact.clone())
            .expect("fact should seed");
        block_on(adapter.ack_consumed(fact.committed_fact_ref.clone(), builder.consumer_marker()))
            .expect("ack should succeed");

        let page = block_on(adapter.poll_committed(builder.origin_cursor(), PageLimit::new(10)))
            .expect("poll should succeed");

        assert!(page.items.is_empty());
        assert_eq!(
            source.acknowledged_marker(&fact.committed_fact_ref),
            Some(builder.consumer_marker())
        );
    }

    #[test]
    fn outbox_source_ack_failure_replays_fact_until_ack_succeeds() {
        let run = TestRunBuilder::new("obx-source-003").build();
        let builder = OutboxFixtureBuilder::new(run);
        let source = SharedOutboxSource::new();
        let adapter = InMemoryOutboxFactSourceAdapter::new(source.clone());
        let fact = builder.committed_fact();

        source
            .seed_committed(fact.clone())
            .expect("fact should seed");
        source.fail_next_ack(SourcePortError::AckFailed);

        let ack_error = block_on(
            adapter.ack_consumed(fact.committed_fact_ref.clone(), builder.consumer_marker()),
        )
        .expect_err("first ack should fail");
        let replay_page =
            block_on(adapter.poll_committed(builder.origin_cursor(), PageLimit::new(10)))
                .expect("fact should replay while unacknowledged");
        block_on(adapter.ack_consumed(fact.committed_fact_ref.clone(), builder.consumer_marker()))
            .expect("second ack should succeed");
        let empty_page =
            block_on(adapter.poll_committed(builder.origin_cursor(), PageLimit::new(10)))
                .expect("acked fact should no longer replay");

        assert_eq!(ack_error, SourcePortError::AckFailed);
        assert_eq!(replay_page.items, vec![fact]);
        assert!(empty_page.items.is_empty());
    }

    #[test]
    fn outbox_source_fixture_rejects_duplicate_fact_ref() {
        let run = TestRunBuilder::new("obx-source-004").build();
        let builder = OutboxFixtureBuilder::new(run);
        let source = SharedOutboxSource::new();
        let fact = builder.committed_fact();

        source
            .seed_committed(fact.clone())
            .expect("first fact should seed");
        let error = source
            .seed_committed(fact.clone())
            .expect_err("duplicate fact ref should fail");

        assert_eq!(
            error,
            OutboxSourceFixtureError::DuplicateFactRef(fact.committed_fact_ref)
        );
    }
}

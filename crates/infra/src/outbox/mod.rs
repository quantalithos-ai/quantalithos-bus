//! In-memory outbound publisher adapters.

mod publisher_memory;

pub use publisher_memory::{
    InMemoryOutboxPublisherAdapter, PublishedEventRecord, SharedPublishedEventSink,
};

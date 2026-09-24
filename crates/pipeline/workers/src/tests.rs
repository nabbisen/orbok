//! Integration tests for orbok-workers (RFC-006/007/008/009).
mod embedding_hybrid;
mod pipeline;

mod v04_features;

mod v05_features;

mod v06_features;

mod v07_features;

mod v08_features;

mod v09_rc;

mod rfc036_scheduler;
mod rfc059_cache_lifetime;
mod rfc059_reset_erasure;
mod task078_repair_cost;
mod task078_requeue_discovered;
mod task095_reset_gives_space_back;
mod task098_reset_and_scheduler;
mod task102_keyword_rebuild_keeps_what_it_does_not_rebuild;
mod v092_features;

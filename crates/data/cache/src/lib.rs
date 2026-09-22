//! # orbok-cache
//!
//! Derived-data cache service over the pinned `localcache` engine
//! (Appendix A). The catalog stays authoritative; everything stored
//! here is rebuildable from source files.

pub mod namespace;
pub mod service;

#[cfg(test)]
mod tests;

pub use namespace::{OrbokCacheNamespace, RETIRED_NAMESPACES};
pub use service::{CacheCleanupOutcome, CacheService, EngineOptions, NamespaceUsage};

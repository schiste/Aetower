use aetower_model::{CapabilityKind, CapabilitySnapshot};

/// Contract for an Aetower adapter.
///
/// Adapters keep external-system context caches fresh and expose the
/// capability snapshot that gates each cache.
///
/// # Lifecycle
///
/// 1. `refresh_cache` is called on the adapter tick (~1 s) when the
///    adapter's capability is `Granted`.
/// 2. `capability_snapshot` is called to report the adapter's status in the
///    settings UI and to decide whether enrichment can consume the cache.
pub trait Adapter: Send + Sync {
    /// The capability kind that gates this adapter.
    fn capability_kind(&self) -> CapabilityKind;

    /// Fetch fresh data from the external system and update internal cache.
    /// Called at the adapter refresh interval when the capability is granted.
    fn refresh_cache(&self);

    /// Return the current capability snapshot for display in settings.
    fn capability_snapshot(&self, last_updated_millis: u64) -> CapabilitySnapshot;
}

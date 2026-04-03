use std::collections::BTreeMap;

use aetower_model::{CapabilityHealth, CapabilityKind, CapabilitySnapshot, EntitySnapshot};

/// Contract for an Aetower adapter.
///
/// Adapters extend entity enrichment by fetching context from external
/// systems (Chromium DevTools, Docker API, Chau7 MCP, etc.) and injecting
/// `ComponentKind::AdapterContext` components and badges into entities.
///
/// # Lifecycle
///
/// 1. `refresh_cache` is called on the adapter tick (~1 s) when the
///    adapter's capability is `Granted`.
/// 2. `enrich_entities` is called on the fast tick (~2 s) with the
///    entity list built by the attribution pipeline.
/// 3. `capability_snapshot` and `capability_health` are called to report
///    the adapter's status in the settings UI.
pub trait Adapter: Send + Sync {
    /// Human-readable adapter name (e.g. "Chromium", "Docker", "Chau7").
    fn name(&self) -> &str;

    /// The capability kind that gates this adapter.
    fn capability_kind(&self) -> CapabilityKind;

    /// Fetch fresh data from the external system and update internal cache.
    /// Called at the adapter refresh interval when the capability is granted.
    fn refresh_cache(&self);

    /// Enrich the given entities with adapter-specific components and badges.
    /// Called on every fast tick after attribution and friction scoring.
    fn enrich_entities(
        &self,
        entities: &mut [EntitySnapshot],
        capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>,
    );

    /// Return the current capability snapshot for display in settings.
    fn capability_snapshot(&self, last_updated_millis: u64) -> CapabilitySnapshot;

    /// Return the adapter's health status.
    fn capability_health(&self) -> CapabilityHealth;
}

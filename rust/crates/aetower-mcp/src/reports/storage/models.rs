use super::*;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StorageHygieneReport {
    pub(super) captured_at_millis: u64,
    pub(super) scan_duration_millis: u64,
    pub(super) scan_mode: String,
    pub(super) diagnostics: StorageScanDiagnostics,
    pub(super) summary: StorageHygieneSummary,
    pub(super) investigation: StorageInvestigationSummary,
    pub(super) cleanup_tiers: Vec<StorageCleanupTierSummary>,
    pub(super) cleanup_recipes: Vec<StorageCleanupRecipe>,
    pub(super) cleanup_bundles: Vec<StorageCleanupBundle>,
    pub(super) budget_guardrails: StorageBudgetGuardrails,
    pub(super) agent_hygiene: StorageAgentHygieneSummary,
    pub(super) repository_inventory: Vec<StorageRepositoryInventoryItem>,
    pub(super) repository_inventory_complete: bool,
    pub(super) repository_inventory_truncated: bool,
    pub(super) repository_inventory_roots: Vec<String>,
    pub(super) repository_inventory_partial_roots: Vec<String>,
    pub(super) repository_inventory_coverage: Vec<RepositoryInventoryRootCoverage>,
    pub(super) repo_footprints: Vec<StorageRepoFootprint>,
    pub(super) duplicate_groups: Vec<StorageDuplicateGroup>,
    pub(super) app_footprints: Vec<StorageAppFootprint>,
    pub(super) system_data_buckets: Vec<StorageSystemDataBucket>,
    pub(super) treemap_roots: Vec<StorageTreemapNode>,
    pub(super) items: Vec<StorageHygieneItem>,
    pub(super) roots: Vec<String>,
    pub(super) skipped_roots: Vec<StorageSkippedRoot>,
    pub(super) source_coverage: Vec<StorageSourceCoverage>,
    pub(super) volume_states: Vec<StorageVolumeState>,
    pub(super) growth_deltas: Vec<StorageGrowthDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) growth_insights: Option<StorageGrowthInsights>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) cold_data: Option<StorageColdData>,
    pub(super) truncated: bool,
    pub(super) caveats: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct StorageHygieneSummary {
    pub(super) item_count: usize,
    pub(super) total_reclaimable_bytes: u64,
    pub(super) safe_candidate_count: usize,
    pub(super) review_candidate_count: usize,
    pub(super) stale_candidate_count: usize,
    pub(super) scanned_directory_count: u64,
    pub(super) largest_item_path: Option<String>,
    pub(super) largest_item_bytes: u64,
    pub(super) attributed_repo_count: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct StorageScanDiagnostics {
    pub(super) mode: String,
    pub(super) root_walk_millis: u64,
    pub(super) size_walk_millis: u64,
    pub(super) git_millis: u64,
    pub(super) serialize_millis: u64,
    pub(super) payload_bytes: u64,
    pub(super) decode_millis: u64,
    pub(super) scanned_directory_count: u64,
    pub(super) discovered_repository_count: u64,
    pub(super) sized_entry_count: u64,
    pub(super) candidate_seen_count: u64,
    pub(super) candidate_retained_count: u64,
    pub(super) storage_index_status: String,
    pub(super) storage_index_hits: u64,
    pub(super) storage_index_misses: u64,
    pub(super) storage_index_writes: u64,
    pub(super) native_metadata_strategy: String,
    pub(super) fsevents_status: String,
    pub(super) lazy_git_status: bool,
    pub(super) top_k_retained: bool,
    pub(super) performance_budget: StoragePerformanceBudgetDiagnostics,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct StoragePerformanceBudgetDiagnostics {
    pub(super) status: String,
    pub(super) scan_job_latency_millis: u64,
    pub(super) payload_bytes: u64,
    pub(super) payload_budget_bytes: u64,
    pub(super) table_page_millis: u64,
    pub(super) table_page_budget_millis: u64,
    pub(super) render_publish_millis: u64,
    pub(super) render_budget_millis: u64,
    pub(super) notes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct StorageScanMetrics {
    pub(super) root_walk_millis: u64,
    pub(super) size_walk_millis: u64,
    pub(super) git_millis: u64,
    pub(super) scanned_directory_count: u64,
    pub(super) discovered_repository_count: u64,
    pub(super) sized_entry_count: u64,
    pub(super) candidate_seen_count: u64,
    pub(super) storage_index_hits: u64,
    pub(super) storage_index_misses: u64,
    pub(super) storage_index_writes: u64,
    pub(super) storage_index_status: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageSourceCoverage {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) kind: String,
    pub(super) path: String,
    pub(super) status: String,
    pub(super) permission_state: String,
    pub(super) gap_kind: String,
    pub(super) detail: String,
    pub(super) local_bytes: Option<u64>,
    pub(super) logical_bytes: Option<u64>,
    pub(super) reclaimable_bytes: Option<u64>,
    pub(super) cloud_placeholder: bool,
    pub(super) network: bool,
    pub(super) protected: bool,
    pub(super) scanned: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct RepositoryInventoryRootCoverage {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) path: String,
    pub(super) status: String,
    pub(super) permission_state: String,
    pub(super) detail: String,
    pub(super) repository_count: u64,
    pub(super) scanned_directory_count: u64,
    pub(super) skipped_directory_count: u64,
    pub(super) truncated: bool,
    pub(super) scanned: bool,
}

#[derive(Clone, Debug)]
pub(super) struct RepositoryInventoryCompleteness {
    pub(super) complete: bool,
    pub(super) truncated: bool,
    pub(super) roots: Vec<String>,
    pub(super) partial_roots: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageVolumeState {
    pub(super) path: String,
    pub(super) device_id: u64,
    pub(super) filesystem_type: String,
    pub(super) total_bytes: u64,
    pub(super) free_now_bytes: u64,
    pub(super) available_bytes: u64,
    pub(super) purgeable_bytes_estimate: u64,
    pub(super) important_usage_available_bytes: Option<u64>,
    pub(super) opportunistic_usage_available_bytes: Option<u64>,
    pub(super) detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageGrowthDelta {
    pub(super) bucket_millis: u64,
    pub(super) scan_millis: u64,
    pub(super) path: String,
    pub(super) source_root: String,
    pub(super) repo_root: Option<String>,
    pub(super) repo_name: Option<String>,
    pub(super) git_branch: Option<String>,
    pub(super) git_head: Option<String>,
    pub(super) kind: String,
    pub(super) cleanup_tier: String,
    pub(super) previous_physical_bytes: u64,
    pub(super) current_physical_bytes: u64,
    pub(super) delta_bytes: i64,
    pub(super) command: Option<String>,
    pub(super) process_tree: Option<String>,
    pub(super) ai_agent_session: Option<String>,
    pub(super) writer_source: Option<String>,
    pub(super) provider: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) tab_name: Option<String>,
    pub(super) chau7_session_id: Option<String>,
    pub(super) writer_display: Option<String>,
    pub(super) matched_writer_count: u64,
    pub(super) matched_filesystem_event_count: u64,
    pub(super) attribution_sources: Vec<String>,
    pub(super) attribution_confidence: String,
    pub(super) attribution_confidence_score: u8,
    pub(super) attribution_ambiguous: bool,
    pub(super) attribution_summary: String,
    pub(super) attribution_evidence: Vec<String>,
}

/// Daily growth rate for one repository or source root, aggregated from the
/// full retained `storage_growth_delta` series (not just the recent event
/// timeline).
#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageGrowthRate {
    pub(super) scope: String,
    pub(super) scope_kind: String,
    pub(super) repo_name: Option<String>,
    pub(super) window_days: u64,
    pub(super) total_delta_bytes: i64,
    pub(super) daily_rate_bytes: i64,
    /// accelerating | steady | slowing | shrinking (half-window comparison).
    pub(super) trend: String,
    pub(super) day_bucket_count: u64,
}

/// Days-to-disk-full forecast for one volume, derived from the aggregate
/// positive daily growth rate over the scanned roots. Only emitted when at
/// least three distinct day buckets of growth history exist.
#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageGrowthForecast {
    pub(super) volume_path: String,
    pub(super) free_now_bytes: u64,
    pub(super) daily_rate_bytes: i64,
    pub(super) days_to_full: f64,
    pub(super) confidence: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageScanDiffEntry {
    pub(super) path: String,
    pub(super) display_name: String,
    pub(super) source_root: String,
    pub(super) repo_root: Option<String>,
    pub(super) kind: String,
    pub(super) cleanup_tier: String,
    pub(super) previous_cleanup_tier: String,
    pub(super) physical_bytes: u64,
    pub(super) delta_bytes: i64,
    pub(super) scan_millis: u64,
}

/// "Since last scan" diff lane derived from the latest delta/index scan
/// generation: newly appeared paths and tier transitions. `disappeared` is
/// intentionally empty (see `disappeared_note`) because unchanged directories
/// are served from the directory size cache and keep prior scan generations,
/// so a stale generation does not imply deletion.
#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct StorageScanDiff {
    pub(super) latest_scan_millis: u64,
    pub(super) appeared_count: u64,
    pub(super) appeared_total_bytes: u64,
    pub(super) appeared: Vec<StorageScanDiffEntry>,
    pub(super) tier_changed_count: u64,
    pub(super) tier_changed: Vec<StorageScanDiffEntry>,
    pub(super) disappeared: Vec<StorageScanDiffEntry>,
    pub(super) disappeared_note: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageGrowthAnomaly {
    pub(super) path: String,
    pub(super) display_name: String,
    pub(super) source_root: String,
    pub(super) repo_root: Option<String>,
    pub(super) repo_name: Option<String>,
    pub(super) kind: String,
    pub(super) cleanup_tier: String,
    pub(super) bucket_millis: u64,
    pub(super) scan_millis: u64,
    pub(super) current_delta_bytes: u64,
    pub(super) baseline_mean_bytes: u64,
    pub(super) baseline_stddev_bytes: u64,
    pub(super) baseline_peak_bytes: u64,
    pub(super) baseline_bucket_count: u64,
    pub(super) current_to_baseline_ratio: f64,
    pub(super) z_score: f64,
    pub(super) severity: String,
    pub(super) confidence: String,
    pub(super) anomaly_kind: String,
    pub(super) summary: String,
    pub(super) evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageGrowthInsights {
    pub(super) window_days: u64,
    pub(super) per_repo_rates: Vec<StorageGrowthRate>,
    pub(super) per_root_rates: Vec<StorageGrowthRate>,
    pub(super) volume_forecasts: Vec<StorageGrowthForecast>,
    pub(super) growth_anomalies: Vec<StorageGrowthAnomaly>,
    pub(super) since_last_scan: StorageScanDiff,
}

#[derive(Debug, Serialize)]
pub(super) struct StorageGrowthInsightsResponse {
    pub(super) captured_at_millis: u64,
    pub(super) roots: Vec<String>,
    pub(super) storage_index_status: String,
    #[serde(flatten)]
    pub(super) insights: StorageGrowthInsights,
}

/// One cold-data reclaim band: rows untouched (max of accessed/modified) for
/// at least `min_age_days`, restricted to safe/rebuildable tiers and the
/// minimum item size, with the largest hydrated items on top.
#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageColdDataBand {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) min_age_days: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_age_days: Option<u64>,
    pub(super) item_count: u64,
    pub(super) total_bytes: u64,
    pub(super) top_items: Vec<StorageHygieneItem>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageColdData {
    pub(super) bands: Vec<StorageColdDataBand>,
    pub(super) caveat: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct StorageWriterLedgerRecord {
    #[serde(default)]
    pub(super) timestamp_millis: Option<u64>,
    #[serde(default)]
    pub(super) started_at_millis: Option<u64>,
    #[serde(default)]
    pub(super) ended_at_millis: Option<u64>,
    #[serde(default)]
    pub(super) path_prefix: Option<String>,
    #[serde(default)]
    pub(super) repo_root: Option<String>,
    #[serde(default)]
    pub(super) repo_name: Option<String>,
    #[serde(default)]
    pub(super) git_branch: Option<String>,
    #[serde(default)]
    pub(super) git_head: Option<String>,
    #[serde(default)]
    pub(super) cwd: Option<String>,
    #[serde(default)]
    pub(super) working_directory: Option<String>,
    #[serde(default)]
    pub(super) command: Option<String>,
    #[serde(default)]
    pub(super) process_tree: Option<String>,
    #[serde(default)]
    pub(super) ai_agent_session: Option<String>,
    #[serde(default)]
    pub(super) provider: Option<String>,
    #[serde(default)]
    pub(super) session_id: Option<String>,
    #[serde(default)]
    pub(super) tab_id: Option<String>,
    #[serde(default)]
    pub(super) tab_name: Option<String>,
    #[serde(default)]
    pub(super) chau7_session_id: Option<String>,
    #[serde(default)]
    pub(super) source: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct StorageFilesystemEventRecord {
    #[serde(default, alias = "event_millis")]
    pub(super) timestamp_millis: Option<u64>,
    #[serde(default)]
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) event_id: Option<u64>,
    #[serde(default)]
    pub(super) flags: Option<u64>,
    #[serde(default)]
    pub(super) source: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct StorageGrowthAttribution {
    pub(super) repo_name: Option<String>,
    pub(super) git_branch: Option<String>,
    pub(super) git_head: Option<String>,
    pub(super) command: Option<String>,
    pub(super) process_tree: Option<String>,
    pub(super) ai_agent_session: Option<String>,
    pub(super) writer_source: Option<String>,
    pub(super) provider: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) tab_name: Option<String>,
    pub(super) chau7_session_id: Option<String>,
    pub(super) writer_display: Option<String>,
    pub(super) matched_writer_count: u64,
    pub(super) matched_filesystem_event_count: u64,
    pub(super) sources: Vec<String>,
    pub(super) confidence: String,
    pub(super) confidence_score: u8,
    pub(super) ambiguous: bool,
    pub(super) summary: String,
    pub(super) evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageHygieneItem {
    pub(super) id: String,
    pub(super) path: String,
    pub(super) display_name: String,
    pub(super) kind: String,
    pub(super) storage_role: String,
    pub(super) git_status: String,
    pub(super) safety: String,
    pub(super) cleanup_tier: String,
    pub(super) size_bytes: u64,
    pub(super) logical_bytes: u64,
    pub(super) physical_bytes: u64,
    pub(super) byte_accounting: String,
    pub(super) sparse_or_shared: bool,
    pub(super) hardlink_count: u64,
    pub(super) has_hardlinks: bool,
    pub(super) cloud_placeholder: bool,
    pub(super) protected_path: bool,
    pub(super) size_truncated: bool,
    pub(super) modified_millis: Option<u64>,
    pub(super) age_days: Option<u64>,
    pub(super) accessed_millis: Option<u64>,
    pub(super) access_age_days: Option<u64>,
    pub(super) cold: bool,
    pub(super) stale: bool,
    pub(super) reason: String,
    pub(super) recommendation: String,
    pub(super) next_step: String,
    pub(super) command_hint: String,
    pub(super) rebuild_command: Option<String>,
    pub(super) estimated_rebuild_cost: String,
    pub(super) estimated_rebuild_seconds: Option<u64>,
    pub(super) cleanup_consequence: String,
    pub(super) evidence: Vec<String>,
    pub(super) cleanup_allowed: bool,
    pub(super) cleanup_blockers: Vec<String>,
    pub(super) default_cleanup_action: String,
    /// Composite reclaim-recommendation score (see
    /// `storage_recommendation_score`); matches the persisted
    /// `recommendation_score` column used for the "score" sort key.
    pub(super) recommendation_score: f64,
    pub(super) attribution: StorageArtifactAttribution,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageTreemapNode {
    pub(super) id: String,
    pub(super) path: String,
    pub(super) label: String,
    pub(super) depth: usize,
    pub(super) node_type: String,
    pub(super) file_type: String,
    pub(super) color_key: String,
    pub(super) size_bytes: u64,
    pub(super) item_count: usize,
    pub(super) children: Vec<StorageTreemapNode>,
    pub(super) has_more: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct StorageInvestigationSummary {
    pub(super) top_findings: Vec<StorageInvestigationFinding>,
    pub(super) known_cache_bytes: u64,
    pub(super) rebuildable_bytes: u64,
    pub(super) expensive_bytes: u64,
    pub(super) risky_bytes: u64,
    pub(super) large_file_count: usize,
    pub(super) cold_file_count: usize,
    pub(super) cold_file_bytes: u64,
    pub(super) review_item_count: usize,
    pub(super) open_conflict_status: String,
    pub(super) recommended_next_steps: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageInvestigationFinding {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) path: String,
    pub(super) storage_role: String,
    pub(super) cleanup_tier: String,
    pub(super) safety: String,
    pub(super) size_bytes: u64,
    pub(super) confidence_score: u8,
    pub(super) evidence: Vec<String>,
    pub(super) recommended_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageArtifactAttribution {
    pub(super) repo_root: Option<String>,
    pub(super) repo_name: Option<String>,
    pub(super) git_branch: Option<String>,
    pub(super) git_head: Option<String>,
    pub(super) command: Option<String>,
    pub(super) process_tree: Option<String>,
    pub(super) ai_agent_session: Option<String>,
    pub(super) provider: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) tab_name: Option<String>,
    pub(super) chau7_session_id: Option<String>,
    pub(super) writer_display: Option<String>,
    pub(super) confidence: String,
    pub(super) notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct StorageCleanupTierSummary {
    pub(super) tier: String,
    pub(super) label: String,
    pub(super) description: String,
    pub(super) item_count: usize,
    pub(super) bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageCleanupRecipe {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) category: String,
    pub(super) safety: String,
    pub(super) affected_path: String,
    pub(super) command: String,
    pub(super) estimated_reclaimable_bytes: u64,
    pub(super) reason: String,
    pub(super) prerequisites: Vec<String>,
    pub(super) destructive: bool,
    pub(super) requires_review: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageCleanupBundle {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) safety: String,
    pub(super) confidence_score: u8,
    pub(super) estimated_reclaimable_bytes: u64,
    pub(super) item_count: usize,
    pub(super) dry_run_only: bool,
    pub(super) manifest: Vec<StorageCleanupBundleItem>,
    pub(super) dry_run_commands: Vec<String>,
    pub(super) rollback_notes: Vec<String>,
    pub(super) prerequisites: Vec<String>,
    pub(super) caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageCleanupBundleItem {
    pub(super) path: String,
    pub(super) display_name: String,
    pub(super) kind: String,
    pub(super) cleanup_tier: String,
    pub(super) safety: String,
    pub(super) size_bytes: u64,
    pub(super) confidence_score: u8,
    pub(super) dry_run_command: String,
    pub(super) cleanup_command: Option<String>,
    pub(super) rollback_note: String,
    pub(super) reason: String,
    pub(super) consequence: String,
    pub(super) evidence: Vec<String>,
    pub(super) cleanup_allowed: bool,
    pub(super) cleanup_blockers: Vec<String>,
    pub(super) default_cleanup_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageBudgetGuardrails {
    pub(super) repo_growth_budget_bytes_per_day: u64,
    pub(super) repo_artifact_budget_bytes: u64,
    pub(super) total_artifact_budget_bytes: u64,
    pub(super) free_space_floor_bytes: u64,
    pub(super) volume_pressure_floor_percent: u64,
    pub(super) warning_only_by_default: bool,
    pub(super) auto_trash_safe_tier_enabled: bool,
    pub(super) scheduled_scan_recommended: bool,
    pub(super) scheduled_scan_interval_hours: u64,
    pub(super) status: String,
    pub(super) violations: Vec<StorageBudgetViolation>,
    pub(super) policies: Vec<StoragePreventionPolicy>,
    pub(super) prevention_suggestions: Vec<StoragePreventionSuggestion>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageBudgetViolation {
    pub(super) id: String,
    pub(super) scope: String,
    pub(super) severity: String,
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) repo_root: Option<String>,
    pub(super) repo_name: Option<String>,
    pub(super) observed_bytes: u64,
    pub(super) limit_bytes: u64,
    pub(super) recommendation: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StoragePreventionPolicy {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) mode: String,
    pub(super) enabled: bool,
    pub(super) action: String,
    pub(super) tier: String,
    pub(super) detail: String,
    pub(super) next_step: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StoragePreventionSuggestion {
    pub(super) id: String,
    pub(super) trigger: String,
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) action_label: String,
    pub(super) estimated_reclaimable_bytes: u64,
    pub(super) safety: String,
    pub(super) requires_approval: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct StorageAgentHygieneSummary {
    pub(super) total_agent_artifact_bytes: u64,
    pub(super) week_agent_artifact_bytes: u64,
    pub(super) rebuildable_agent_bytes: u64,
    pub(super) rebuildable_agent_percent: f32,
    pub(super) week_rebuildable_agent_bytes: u64,
    pub(super) week_rebuildable_agent_percent: f32,
    pub(super) attributed_item_count: usize,
    pub(super) agent_count: usize,
    pub(super) agents: Vec<StorageAgentArtifactSummary>,
    pub(super) caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageAgentArtifactSummary {
    pub(super) id: String,
    pub(super) provider: String,
    pub(super) display_name: String,
    pub(super) session_id: Option<String>,
    pub(super) artifact_bytes: u64,
    pub(super) week_artifact_bytes: u64,
    pub(super) rebuildable_bytes: u64,
    pub(super) rebuildable_percent: f32,
    pub(super) week_rebuildable_bytes: u64,
    pub(super) week_rebuildable_percent: f32,
    pub(super) item_count: usize,
    pub(super) repo_count: usize,
    pub(super) top_repositories: Vec<StorageAgentRepoSummary>,
    pub(super) top_items: Vec<StorageAgentItemSummary>,
    pub(super) confidence: String,
    pub(super) attribution_sources: Vec<String>,
    pub(super) recommendation: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageAgentRepoSummary {
    pub(super) repo_root: String,
    pub(super) repo_name: String,
    pub(super) artifact_bytes: u64,
    pub(super) item_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageAgentItemSummary {
    pub(super) path: String,
    pub(super) display_name: String,
    pub(super) kind: String,
    pub(super) cleanup_tier: String,
    pub(super) size_bytes: u64,
    pub(super) modified_millis: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageDuplicateGroup {
    pub(super) id: String,
    pub(super) candidate_key: String,
    pub(super) confirmed: bool,
    pub(super) confidence_score: u8,
    pub(super) file_count: usize,
    pub(super) total_bytes: u64,
    pub(super) reclaimable_bytes: u64,
    pub(super) paths: Vec<StorageDuplicateItem>,
    pub(super) recommendation: String,
    pub(super) caveat: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageDuplicateItem {
    pub(super) path: String,
    pub(super) display_name: String,
    pub(super) size_bytes: u64,
    pub(super) modified_millis: Option<u64>,
    pub(super) cleanup_tier: String,
    pub(super) safety: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageAppFootprint {
    pub(super) id: String,
    pub(super) app_name: String,
    pub(super) bundle_identifier: Option<String>,
    pub(super) total_bytes: u64,
    pub(super) component_count: usize,
    pub(super) cleanup_tier: String,
    pub(super) safety: String,
    pub(super) confidence_score: u8,
    pub(super) ownership_status: String,
    pub(super) orphan_confidence: String,
    pub(super) ownership_signals: Vec<StorageAppOwnershipSignal>,
    pub(super) orphan_recommendation: String,
    pub(super) components: Vec<StorageAppFootprintComponent>,
    pub(super) recommendation: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageAppOwnershipSignal {
    pub(super) source: String,
    pub(super) status: String,
    pub(super) detail: String,
    pub(super) path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageAppFootprintComponent {
    pub(super) path: String,
    pub(super) component: String,
    pub(super) size_bytes: u64,
    pub(super) cleanup_tier: String,
    pub(super) safety: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageSystemDataBucket {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) category: String,
    pub(super) size_bytes: u64,
    pub(super) item_count: usize,
    pub(super) cleanup_tier: String,
    pub(super) safety: String,
    pub(super) explanation: String,
    pub(super) recommended_action: String,
    pub(super) paths: Vec<String>,
    pub(super) requires_full_disk_access: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageRepoFootprint {
    pub(super) id: String,
    pub(super) repo_root: String,
    pub(super) repo_name: String,
    pub(super) current_size_bytes: u64,
    pub(super) artifact_bytes: u64,
    pub(super) safe_bytes: u64,
    pub(super) rebuildable_bytes: u64,
    pub(super) expensive_bytes: u64,
    pub(super) risky_bytes: u64,
    pub(super) rebuildable_percent: f32,
    pub(super) item_count: usize,
    pub(super) top_artifact_folders: Vec<StorageRepoArtifactFolder>,
    pub(super) artifact_mix: Vec<StorageRepoArtifactMix>,
    pub(super) duplicate_clone_count: u64,
    pub(super) duplicate_clone_roots: Vec<String>,
    pub(super) last_writer_process: Option<String>,
    pub(super) last_writer_pid: Option<u32>,
    pub(super) last_branch_touched: Option<String>,
    pub(super) growth_bytes: Option<i64>,
    pub(super) growth_window: String,
    pub(super) estimated_rebuild_cost: String,
    pub(super) estimated_rebuild_seconds: Option<u64>,
    pub(super) optimization_summary: String,
    pub(super) caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageRepositoryInventoryItem {
    pub(super) id: String,
    pub(super) repo_root: String,
    pub(super) repo_name: String,
    pub(super) inventory_cache_status: String,
    pub(super) inventory_fingerprint: String,
    pub(super) inventory_fingerprint_changed: bool,
    pub(super) inventory_last_seen_millis: Option<u64>,
    pub(super) inventory_last_scan_millis: Option<u64>,
    pub(super) git_branch: Option<String>,
    pub(super) git_head: Option<String>,
    pub(super) git_ref: Option<String>,
    pub(super) git_detached_head: bool,
    pub(super) git_remote_origin_url: Option<String>,
    pub(super) git_remote_key: Option<String>,
    pub(super) git_remote_host: Option<String>,
    pub(super) git_remote_owner: Option<String>,
    pub(super) git_remote_name: Option<String>,
    pub(super) git_dirty_status: String,
    pub(super) git_dirty_file_count: Option<u64>,
    pub(super) git_dirty_truncated: bool,
    pub(super) not_seen_in_latest_scan: bool,
    pub(super) clone_group_count: u64,
    pub(super) clone_group_roots: Vec<String>,
    pub(super) discovered_root: String,
    pub(super) has_agents_md: bool,
    pub(super) has_claude_md: bool,
    pub(super) claude_md_bytes: Option<u64>,
    pub(super) claude_md_delegation_max_bytes: u64,
    pub(super) claude_md_delegates_to_agents_md: bool,
    pub(super) agent_readiness_score: u8,
    pub(super) agent_readiness_status: String,
    pub(super) agent_contract_missing_count: u64,
    pub(super) agent_contract_coverage: Vec<StorageAgentContractCoverage>,
    pub(super) agent_guidance_status: String,
    pub(super) agent_guidance_issue_count: u64,
    pub(super) agent_guidance_issues: Vec<StorageAgentGuidanceIssue>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageAgentContractCoverage {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) path: String,
    pub(super) kind: String,
    pub(super) status: String,
    pub(super) severity: String,
    pub(super) detail: String,
    pub(super) weight: u64,
    pub(super) earned_weight: u64,
    pub(super) coverage_percent: u8,
    pub(super) present: bool,
    pub(super) tracked: bool,
    pub(super) schema_version: Option<String>,
    pub(super) generated: bool,
    pub(super) reviewed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageAgentGuidanceIssue {
    pub(super) id: String,
    pub(super) severity: String,
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) path: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageRepoArtifactFolder {
    pub(super) path: String,
    pub(super) display_name: String,
    pub(super) kind: String,
    pub(super) cleanup_tier: String,
    pub(super) size_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageRepoArtifactMix {
    pub(super) kind: String,
    pub(super) label: String,
    pub(super) item_count: usize,
    pub(super) bytes: u64,
    pub(super) cleanup_tier: String,
    pub(super) rebuild_command: Option<String>,
    pub(super) estimated_rebuild_cost: String,
    pub(super) estimated_rebuild_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageSkippedRoot {
    pub(super) path: String,
    pub(super) reason: String,
}

#[derive(Clone, Debug)]
pub(super) struct StorageHygieneOptions {
    pub(super) max_depth: usize,
    pub(super) limit: usize,
    pub(super) mode: StorageScanMode,
    pub(super) runtime: Option<StorageScanRuntimeContext>,
    pub(super) dirty_paths: Vec<String>,
}

#[derive(Clone, Copy)]
pub(super) enum StorageItemSortKey {
    Size,
    Path,
    Modified,
    Accessed,
    Tier,
    Kind,
    Score,
}

impl StorageItemSortKey {
    pub(super) fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "path" | "name" => Self::Path,
            "modified" | "mtime" | "newest" | "oldest" => Self::Modified,
            "accessed" | "atime" | "unused" => Self::Accessed,
            "tier" | "cleanup_tier" | "safety" => Self::Tier,
            "kind" | "type" => Self::Kind,
            "score" | "recommended" | "recommendation" => Self::Score,
            _ => Self::Size,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Path => "path",
            Self::Modified => "modified",
            Self::Accessed => "accessed",
            Self::Tier => "tier",
            Self::Kind => "kind",
            Self::Score => "score",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageHygieneOverviewResponse {
    pub(super) captured_at_millis: u64,
    pub(super) scan_duration_millis: u64,
    pub(super) scan_mode: String,
    pub(super) diagnostics: StorageScanDiagnostics,
    pub(super) summary: StorageHygieneSummary,
    pub(super) investigation: StorageInvestigationSummary,
    pub(super) cleanup_tiers: Vec<StorageCleanupTierSummary>,
    pub(super) cleanup_recipes: Vec<StorageCleanupRecipe>,
    pub(super) cleanup_bundles: Vec<StorageCleanupBundle>,
    pub(super) budget_guardrails: StorageBudgetGuardrails,
    pub(super) agent_hygiene: StorageAgentHygieneSummary,
    pub(super) repository_inventory_complete: bool,
    pub(super) repository_inventory_truncated: bool,
    pub(super) repository_inventory_roots: Vec<String>,
    pub(super) repository_inventory_partial_roots: Vec<String>,
    pub(super) repository_inventory_coverage: Vec<RepositoryInventoryRootCoverage>,
    pub(super) repo_footprints: Vec<StorageRepoFootprint>,
    pub(super) duplicate_groups: Vec<StorageDuplicateGroup>,
    pub(super) app_footprints: Vec<StorageAppFootprint>,
    pub(super) system_data_buckets: Vec<StorageSystemDataBucket>,
    pub(super) treemap_roots: Vec<StorageTreemapNode>,
    pub(super) items: Vec<StorageHygieneItem>,
    pub(super) roots: Vec<String>,
    pub(super) skipped_roots: Vec<StorageSkippedRoot>,
    pub(super) source_coverage: Vec<StorageSourceCoverage>,
    pub(super) volume_states: Vec<StorageVolumeState>,
    pub(super) growth_deltas: Vec<StorageGrowthDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) growth_insights: Option<StorageGrowthInsights>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) cold_data: Option<StorageColdData>,
    pub(super) truncated: bool,
    pub(super) caveats: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RepositoryInventoryReport {
    pub(super) captured_at_millis: u64,
    pub(super) scan_duration_millis: u64,
    pub(super) roots: Vec<String>,
    pub(super) repository_inventory: Vec<StorageRepositoryInventoryItem>,
    pub(super) repository_inventory_complete: bool,
    pub(super) repository_inventory_truncated: bool,
    pub(super) repository_inventory_roots: Vec<String>,
    pub(super) repository_inventory_partial_roots: Vec<String>,
    pub(super) repository_inventory_coverage: Vec<RepositoryInventoryRootCoverage>,
    pub(super) truncated: bool,
    pub(super) diagnostics: RepositoryInventoryDiagnostics,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct RepositoryInventoryDiagnostics {
    pub(super) repository_walk_millis: u64,
    pub(super) git_millis: u64,
    pub(super) discovered_repository_count: u64,
    pub(super) scanned_directory_count: u64,
    pub(super) skipped_directory_count: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageHygieneActionsResponse {
    pub(super) captured_at_millis: u64,
    pub(super) scan_mode: String,
    pub(super) diagnostics: StorageScanDiagnostics,
    pub(super) cleanup_tiers: Vec<StorageCleanupTierSummary>,
    pub(super) cleanup_recipes: Vec<StorageCleanupRecipe>,
    pub(super) cleanup_bundles: Vec<StorageCleanupBundle>,
    pub(super) budget_guardrails: StorageBudgetGuardrails,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageHygieneItemsPageResponse {
    pub(super) captured_at_millis: u64,
    pub(super) scan_mode: String,
    pub(super) diagnostics: StorageScanDiagnostics,
    pub(super) offset: usize,
    pub(super) limit: usize,
    pub(super) sort_key: String,
    pub(super) sort_descending: bool,
    pub(super) returned_count: usize,
    pub(super) total_available: usize,
    pub(super) has_more: bool,
    pub(super) page_source: String,
    pub(super) items: Vec<StorageHygieneItem>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct StorageHygieneRepoDetailResponse {
    pub(super) captured_at_millis: u64,
    pub(super) scan_mode: String,
    pub(super) diagnostics: StorageScanDiagnostics,
    pub(super) repository: Option<StorageRepositoryInventoryItem>,
    pub(super) repo_footprints: Vec<StorageRepoFootprint>,
    pub(super) items: Vec<StorageHygieneItem>,
    pub(super) cleanup_recipes: Vec<StorageCleanupRecipe>,
    pub(super) cleanup_bundles: Vec<StorageCleanupBundle>,
    pub(super) caveats: Vec<String>,
}

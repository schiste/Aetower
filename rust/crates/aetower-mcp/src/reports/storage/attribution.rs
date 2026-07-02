use super::*;

pub(super) fn attribute_storage_growth_delta(
    delta: &StorageGrowthDelta,
    writer_ledger: &[StorageWriterLedgerRecord],
) -> StorageGrowthAttribution {
    let repo_root = delta.repo_root.as_deref().map(Path::new);
    let git_head = repo_root.map(read_git_head).unwrap_or_default();
    let repo_name = delta.repo_root.as_deref().and_then(|root| {
        Path::new(root)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    });
    let inferred_agent_session = known_agent_path(Path::new(&delta.path))
        .map(|(_, display_name)| format!("{display_name} local artifacts"));
    let mut sources = vec!["index_delta".to_owned()];
    let matching_records = writer_ledger
        .iter()
        .filter(|record| storage_writer_record_matches_delta(record, delta))
        .collect::<Vec<_>>();
    let mut evidence = vec![format!(
        "Indexed growth delta: {} -> {} bytes.",
        delta.previous_physical_bytes, delta.current_physical_bytes
    )];

    if let Some(repo_root) = delta.repo_root.as_deref() {
        evidence.push(format!("Repo matched by indexed path: {repo_root}."));
        sources.push("repo_path".to_owned());
    }
    if let Some(session) = inferred_agent_session.as_deref() {
        evidence.push(format!("AI-agent directory evidence: {session}."));
        sources.push("agent_path".to_owned());
    }

    if matching_records.len() == 1 {
        let record = matching_records[0];
        let command = first_non_empty(record.command.as_deref(), None);
        let process_tree = first_non_empty(record.process_tree.as_deref(), None);
        let derived_agent_session = derived_writer_agent_session(record);
        let ai_agent_session = first_non_empty(
            record.ai_agent_session.as_deref(),
            derived_agent_session.as_deref(),
        );
        let ai_agent_session = first_non_empty(
            ai_agent_session.as_deref(),
            inferred_agent_session.as_deref(),
        );
        let writer_source = writer_record_source(record);
        if let Some(source) = record.source.as_deref().filter(|value| !value.is_empty()) {
            evidence.push(format!("Writer ledger source: {source}."));
        }
        if let Some(source) = writer_source.as_deref() {
            sources.push(source.to_owned());
        }
        sources.push("writer_ledger".to_owned());
        if let Some(prefix) = writer_record_path_prefixes(record).first() {
            evidence.push(format!("Writer ledger path prefix matched: {prefix}."));
        }
        if let Some(command) = command.as_deref() {
            evidence.push(format!("Command matched: {command}."));
            sources.push("command".to_owned());
        }
        if let Some(process_tree) = process_tree.as_deref() {
            evidence.push(format!("Process tree matched: {process_tree}."));
            sources.push("process_tree".to_owned());
        }
        if let Some(session) = ai_agent_session.as_deref() {
            evidence.push(format!("AI session matched: {session}."));
            sources.push("ai_session".to_owned());
        }

        return StorageGrowthAttribution {
            repo_name: first_non_empty(record.repo_name.as_deref(), repo_name.as_deref()),
            git_branch: first_non_empty(record.git_branch.as_deref(), git_head.branch.as_deref()),
            git_head: first_non_empty(record.git_head.as_deref(), git_head.short_head.as_deref()),
            command,
            process_tree,
            ai_agent_session,
            writer_source,
            matched_writer_count: 1,
            sources: unique_limited(sources, 12),
            confidence: "high".to_owned(),
            confidence_score: 92,
            ambiguous: false,
            summary: "Single writer ledger record matched this growth window.".to_owned(),
            evidence,
        };
    }

    if matching_records.len() > 1 {
        let candidate_labels = matching_records
            .iter()
            .take(4)
            .map(|record| {
                record
                    .command
                    .as_deref()
                    .or(record.ai_agent_session.as_deref())
                    .or(record.process_tree.as_deref())
                    .or(record.source.as_deref())
                    .unwrap_or("unknown writer")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        evidence.push(format!(
            "{} writer ledger records overlapped this path/time window: {}.",
            matching_records.len(),
            candidate_labels.join(", ")
        ));
        sources.push("writer_ledger".to_owned());
        sources.push("ambiguous_writers".to_owned());
        return StorageGrowthAttribution {
            repo_name,
            git_branch: git_head.branch,
            git_head: git_head.short_head,
            command: None,
            process_tree: None,
            ai_agent_session: inferred_agent_session,
            writer_source: None,
            matched_writer_count: matching_records.len().min(u64::MAX as usize) as u64,
            sources: unique_limited(sources, 12),
            confidence: "ambiguous".to_owned(),
            confidence_score: 45,
            ambiguous: true,
            summary: "Multiple writer records overlapped; Aetower will not pick a single culprit."
                .to_owned(),
            evidence,
        };
    }

    let (confidence, confidence_score, summary) = if delta.repo_root.is_some()
        && inferred_agent_session.is_some()
    {
        (
            "medium",
            74,
            "Repo and AI-agent directory evidence matched, but no command writer record was available.",
        )
    } else if delta.repo_root.is_some() {
        (
            "medium",
            64,
            "Repo/branch attribution is available from the indexed path, but command/session writer evidence is missing.",
        )
    } else if inferred_agent_session.is_some() {
        (
            "medium",
            58,
            "AI-agent directory evidence matched, but no repo or command writer record was available.",
        )
    } else {
        (
            "low",
            25,
            "Only an indexed storage delta is available; no repo, command, process tree, or AI session matched.",
        )
    };
    evidence.push("No matching writer ledger record was available for this delta.".to_owned());

    StorageGrowthAttribution {
        repo_name,
        git_branch: git_head.branch,
        git_head: git_head.short_head,
        command: None,
        process_tree: None,
        ai_agent_session: inferred_agent_session,
        writer_source: None,
        matched_writer_count: 0,
        sources: unique_limited(sources, 12),
        confidence: confidence.to_owned(),
        confidence_score,
        ambiguous: false,
        summary: summary.to_owned(),
        evidence,
    }
}

fn storage_writer_record_matches_delta(
    record: &StorageWriterLedgerRecord,
    delta: &StorageGrowthDelta,
) -> bool {
    let path_matches = writer_record_path_prefixes(record)
        .iter()
        .any(|prefix| delta.path == *prefix || delta.path.starts_with(&format!("{prefix}/")));
    let repo_matches = record
        .repo_root
        .as_deref()
        .zip(delta.repo_root.as_deref())
        .is_some_and(|(record_repo, delta_repo)| record_repo == delta_repo);
    if !path_matches && !repo_matches {
        return false;
    }
    storage_writer_record_time_matches(record, delta.scan_millis, delta.bucket_millis)
}

fn writer_record_path_prefixes(record: &StorageWriterLedgerRecord) -> Vec<String> {
    unique_limited(
        [
            record.path_prefix.as_deref(),
            record.working_directory.as_deref(),
            record.cwd.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter_map(normalized_writer_path_prefix)
        .collect(),
        4,
    )
}

fn normalized_writer_path_prefix(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.trim_end_matches('/').to_owned())
}

fn writer_record_source(record: &StorageWriterLedgerRecord) -> Option<String> {
    first_non_empty(record.source.as_deref(), record.provider.as_deref())
}

fn derived_writer_agent_session(record: &StorageWriterLedgerRecord) -> Option<String> {
    first_non_empty(
        record.ai_agent_session.as_deref(),
        record
            .session_id
            .as_deref()
            .or(record.chau7_session_id.as_deref())
            .or(record.tab_id.as_deref())
            .or(record.tab_name.as_deref()),
    )
    .map(|session| {
        if let Some(provider) = record.provider.as_deref().filter(|value| !value.is_empty()) {
            format!("{provider} session {session}")
        } else if record
            .source
            .as_deref()
            .is_some_and(|source| source.eq_ignore_ascii_case("chau7"))
        {
            format!("Chau7 session {session}")
        } else {
            session
        }
    })
}

fn storage_writer_record_time_matches(
    record: &StorageWriterLedgerRecord,
    scan_millis: u64,
    bucket_millis: u64,
) -> bool {
    if let Some(started) = record.started_at_millis {
        let ended = record.ended_at_millis.unwrap_or(scan_millis);
        return scan_millis.saturating_add(STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS) >= started
            && ended.saturating_add(STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS) >= bucket_millis;
    }
    if let Some(timestamp) = record.timestamp_millis {
        let lower = bucket_millis.saturating_sub(STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS);
        let upper = scan_millis.saturating_add(STORAGE_WRITER_LEDGER_TIME_FUZZ_MILLIS);
        return (lower..=upper).contains(&timestamp);
    }
    true
}

pub(super) fn load_storage_writer_ledger_records() -> Vec<StorageWriterLedgerRecord> {
    storage_writer_ledger_paths()
        .into_iter()
        .flat_map(|path| load_storage_writer_ledger_records_from_path(&path))
        .take(512)
        .collect()
}

fn storage_writer_ledger_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(path) = std::env::var("AETOWER_STORAGE_WRITER_LEDGER")
        && !path.trim().is_empty()
    {
        paths.push(PathBuf::from(path));
    }
    if let Some(base_dir) = dirs::data_local_dir() {
        paths.push(
            base_dir
                .join("Aetower")
                .join("storage-writer-ledger.ndjson"),
        );
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".aetower").join("storage-writer-ledger.ndjson"));
        paths.push(home.join(".chau7").join("storage-writer-ledger.ndjson"));
    }
    paths
}

fn load_storage_writer_ledger_records_from_path(path: &Path) -> Vec<StorageWriterLedgerRecord> {
    let Ok(metadata) = fs::metadata(path) else {
        return Vec::new();
    };
    if !metadata.is_file() || metadata.len() > STORAGE_WRITER_LEDGER_MAX_BYTES {
        return Vec::new();
    }
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .rev()
        .take(512)
        .filter_map(|line| serde_json::from_str::<StorageWriterLedgerRecord>(line).ok())
        .collect()
}

#[derive(Clone, Debug)]
struct AgentAttributionEvidence {
    id: String,
    provider: String,
    display_name: String,
    session_id: Option<String>,
    confidence: String,
    source: String,
}

#[derive(Clone, Debug)]
struct AgentArtifactAccumulator {
    id: String,
    provider: String,
    display_name: String,
    session_id: Option<String>,
    artifact_bytes: u64,
    week_artifact_bytes: u64,
    rebuildable_bytes: u64,
    week_rebuildable_bytes: u64,
    item_count: usize,
    repos: BTreeMap<String, AgentRepoAccumulator>,
    top_items: Vec<StorageAgentItemSummary>,
    confidence: String,
    confidence_rank: u8,
    attribution_sources: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
struct AgentRepoAccumulator {
    repo_root: String,
    repo_name: String,
    artifact_bytes: u64,
    item_count: usize,
}

impl AgentArtifactAccumulator {
    fn new(evidence: AgentAttributionEvidence) -> Self {
        let mut attribution_sources = BTreeSet::new();
        attribution_sources.insert(evidence.source);
        let confidence_rank = confidence_rank(&evidence.confidence);
        Self {
            id: evidence.id,
            provider: evidence.provider,
            display_name: evidence.display_name,
            session_id: evidence.session_id,
            artifact_bytes: 0,
            week_artifact_bytes: 0,
            rebuildable_bytes: 0,
            week_rebuildable_bytes: 0,
            item_count: 0,
            repos: BTreeMap::new(),
            top_items: Vec::new(),
            confidence: evidence.confidence,
            confidence_rank,
            attribution_sources,
        }
    }

    fn merge_evidence(&mut self, evidence: AgentAttributionEvidence) {
        self.attribution_sources.insert(evidence.source);
        let rank = confidence_rank(&evidence.confidence);
        if rank > self.confidence_rank {
            self.confidence = evidence.confidence;
            self.confidence_rank = rank;
        }
        if self.session_id.is_none() {
            self.session_id = evidence.session_id;
        }
    }

    fn add_item(&mut self, item: &StorageHygieneItem) {
        self.artifact_bytes = self.artifact_bytes.saturating_add(item.size_bytes);
        self.item_count += 1;
        let is_rebuildable = item.cleanup_tier == "rebuildable";
        if is_rebuildable {
            self.rebuildable_bytes = self.rebuildable_bytes.saturating_add(item.size_bytes);
        }
        if item.age_days.is_some_and(|days| days <= 7) {
            self.week_artifact_bytes = self.week_artifact_bytes.saturating_add(item.size_bytes);
            if is_rebuildable {
                self.week_rebuildable_bytes =
                    self.week_rebuildable_bytes.saturating_add(item.size_bytes);
            }
        }

        if let Some(repo_root) = item.attribution.repo_root.as_deref() {
            let repo =
                self.repos
                    .entry(repo_root.to_owned())
                    .or_insert_with(|| AgentRepoAccumulator {
                        repo_root: repo_root.to_owned(),
                        repo_name: item
                            .attribution
                            .repo_name
                            .clone()
                            .or_else(|| {
                                Path::new(repo_root)
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .map(str::to_owned)
                            })
                            .unwrap_or_else(|| "repository".to_owned()),
                        ..Default::default()
                    });
            repo.artifact_bytes = repo.artifact_bytes.saturating_add(item.size_bytes);
            repo.item_count += 1;
        }

        self.top_items.push(StorageAgentItemSummary {
            path: item.path.clone(),
            display_name: item.display_name.clone(),
            kind: item.kind.clone(),
            cleanup_tier: item.cleanup_tier.clone(),
            size_bytes: item.size_bytes,
            modified_millis: item.modified_millis,
        });
    }

    fn into_summary(mut self) -> StorageAgentArtifactSummary {
        let repo_count = self.repos.len();
        let mut top_repositories: Vec<_> = self
            .repos
            .into_values()
            .map(|repo| StorageAgentRepoSummary {
                repo_root: repo.repo_root,
                repo_name: repo.repo_name,
                artifact_bytes: repo.artifact_bytes,
                item_count: repo.item_count,
            })
            .collect();
        top_repositories.sort_by(|left, right| {
            right
                .artifact_bytes
                .cmp(&left.artifact_bytes)
                .then_with(|| left.repo_name.cmp(&right.repo_name))
        });
        top_repositories.truncate(4);

        self.top_items.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        self.top_items.truncate(4);

        StorageAgentArtifactSummary {
            id: self.id,
            provider: self.provider,
            display_name: self.display_name,
            session_id: self.session_id,
            artifact_bytes: self.artifact_bytes,
            week_artifact_bytes: self.week_artifact_bytes,
            rebuildable_bytes: self.rebuildable_bytes,
            rebuildable_percent: percent(self.rebuildable_bytes, self.artifact_bytes),
            week_rebuildable_bytes: self.week_rebuildable_bytes,
            week_rebuildable_percent: percent(
                self.week_rebuildable_bytes,
                self.week_artifact_bytes,
            ),
            item_count: self.item_count,
            repo_count,
            top_repositories,
            top_items: self.top_items,
            confidence: self.confidence,
            attribution_sources: self.attribution_sources.into_iter().collect(),
            recommendation: agent_cleanup_recommendation(
                self.artifact_bytes,
                self.rebuildable_bytes,
                self.week_artifact_bytes,
            ),
        }
    }
}

pub(super) fn summarize_agent_hygiene(items: &[StorageHygieneItem]) -> StorageAgentHygieneSummary {
    let mut grouped = BTreeMap::<String, AgentArtifactAccumulator>::new();

    for item in items {
        let Some(evidence) = infer_agent_evidence(item) else {
            continue;
        };
        grouped
            .entry(evidence.id.clone())
            .and_modify(|entry| entry.merge_evidence(evidence.clone()))
            .or_insert_with(|| AgentArtifactAccumulator::new(evidence))
            .add_item(item);
    }

    let mut agents: Vec<_> = grouped
        .into_values()
        .map(AgentArtifactAccumulator::into_summary)
        .collect();
    agents.sort_by(|left, right| {
        right
            .week_artifact_bytes
            .cmp(&left.week_artifact_bytes)
            .then_with(|| right.artifact_bytes.cmp(&left.artifact_bytes))
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    agents.truncate(12);

    let total_agent_artifact_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.artifact_bytes)
    });
    let week_agent_artifact_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.week_artifact_bytes)
    });
    let rebuildable_agent_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.rebuildable_bytes)
    });
    let week_rebuildable_agent_bytes = agents.iter().fold(0u64, |total, agent| {
        total.saturating_add(agent.week_rebuildable_bytes)
    });
    let attributed_item_count = agents.iter().fold(0usize, |total, agent| {
        total.saturating_add(agent.item_count)
    });

    StorageAgentHygieneSummary {
        total_agent_artifact_bytes,
        week_agent_artifact_bytes,
        rebuildable_agent_bytes,
        rebuildable_agent_percent: percent(rebuildable_agent_bytes, total_agent_artifact_bytes),
        week_rebuildable_agent_bytes,
        week_rebuildable_agent_percent: percent(
            week_rebuildable_agent_bytes,
            week_agent_artifact_bytes,
        ),
        attributed_item_count,
        agent_count: agents.len(),
        agents,
        caveats: vec![
            "Agent cost is direct attribution only: explicit AI session metadata, command/process-tree evidence, or known local agent directories."
                .to_owned(),
            "Repository build artifacts are not blamed on an agent unless Aetower has writer evidence."
                .to_owned(),
        ],
    }
}

fn infer_agent_evidence(item: &StorageHygieneItem) -> Option<AgentAttributionEvidence> {
    if let Some(session) = item.attribution.ai_agent_session.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(session)
    {
        let inferred_local_artifacts = session.ends_with(" local artifacts");
        return Some(AgentAttributionEvidence {
            id: if inferred_local_artifacts {
                format!("known-path|{provider}")
            } else {
                format!("session|{}|{}", provider, session)
            },
            provider,
            display_name,
            session_id: Some(session.to_owned()),
            confidence: if inferred_local_artifacts {
                "medium".to_owned()
            } else {
                "high".to_owned()
            },
            source: if inferred_local_artifacts {
                "known_agent_directory".to_owned()
            } else {
                "ai_agent_session".to_owned()
            },
        });
    }

    if let Some(command) = item.attribution.command.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(command)
    {
        return Some(AgentAttributionEvidence {
            id: format!("command|{provider}"),
            provider,
            display_name,
            session_id: None,
            confidence: "medium".to_owned(),
            source: "command".to_owned(),
        });
    }

    if let Some(process_tree) = item.attribution.process_tree.as_deref()
        && let Some((provider, display_name)) = agent_provider_from_text(process_tree)
    {
        return Some(AgentAttributionEvidence {
            id: format!("process-tree|{provider}"),
            provider,
            display_name,
            session_id: None,
            confidence: "medium".to_owned(),
            source: "process_tree".to_owned(),
        });
    }

    let path = Path::new(&item.path);
    known_agent_path(path).map(|(provider, display_name)| AgentAttributionEvidence {
        id: format!("known-path|{provider}"),
        provider,
        display_name,
        session_id: None,
        confidence: "medium".to_owned(),
        source: "known_agent_directory".to_owned(),
    })
}

fn agent_provider_from_text(value: &str) -> Option<(String, String)> {
    let lowered = value.to_ascii_lowercase();
    if lowered.contains("claude") {
        return Some(("claude".to_owned(), "Claude Code".to_owned()));
    }
    if lowered.contains("codex") {
        return Some(("codex".to_owned(), "Codex".to_owned()));
    }
    if lowered.contains("cursor-agent") || lowered.contains("cursor") {
        return Some(("cursor".to_owned(), "Cursor".to_owned()));
    }
    if lowered.contains("aider") {
        return Some(("aider".to_owned(), "Aider".to_owned()));
    }
    None
}

pub(super) fn known_agent_path(path: &Path) -> Option<(String, String)> {
    for component in path.components() {
        let normalized = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        match normalized.as_str() {
            ".claude" => return Some(("claude".to_owned(), "Claude Code".to_owned())),
            ".codex" => return Some(("codex".to_owned(), "Codex".to_owned())),
            ".cursor" => return Some(("cursor".to_owned(), "Cursor".to_owned())),
            ".aider" => return Some(("aider".to_owned(), "Aider".to_owned())),
            _ => {}
        }
    }
    None
}

fn agent_cleanup_recommendation(
    artifact_bytes: u64,
    rebuildable_bytes: u64,
    week_artifact_bytes: u64,
) -> String {
    if artifact_bytes == 0 {
        return "No agent-attributed storage pressure detected.".to_owned();
    }
    let rebuildable_percent = percent(rebuildable_bytes, artifact_bytes);
    if rebuildable_percent >= 75.0 {
        return "Most attributed bytes are rebuildable; clean them after the agent session and related builds are idle.".to_owned();
    }
    if week_artifact_bytes >= REPO_GROWTH_BUDGET_BYTES_PER_DAY {
        return "This agent added significant storage this week; inspect the top items before starting more build-heavy tasks.".to_owned();
    }
    "Review top items and prefer targeted cleanup recipes over broad cache deletion.".to_owned()
}

fn confidence_rank(confidence: &str) -> u8 {
    match confidence {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

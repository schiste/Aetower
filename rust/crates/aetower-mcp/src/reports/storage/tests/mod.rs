use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn storage_hygiene_detects_reclaimable_build_artifacts() {
    let root = test_root("detects-build-artifacts");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }
    mark_tree_old(&root.join("project"));

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"kind\":\"rust-build\""));
    assert!(json.contains("\"safety\":\"safe\""));
    assert!(json.contains("\"cleanup_tier\":\"rebuildable\""));
    assert!(json.contains("\"cleanup_tiers\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_fast_mode_uses_lazy_git_status_and_diagnostics() {
    let root = test_root("fast-mode-diagnostics");
    let project = root.join("Aetower");
    let target = project.join("target").join("debug");
    create_git_repo(&project, "master");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let json = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );
    let value = parse_json_value(&json, "storage JSON parses");

    assert_eq!(value["scan_mode"], "fast_changed_only");
    assert_eq!(value["diagnostics"]["lazy_git_status"], true);
    assert_eq!(value["diagnostics"]["top_k_retained"], true);
    assert_eq!(
        value["repository_inventory"][0]["git_dirty_status"],
        "not_checked_lazy"
    );
    assert_eq!(
        value["repository_inventory_coverage"][0]["repository_count"].as_u64(),
        Some(1)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_projection_apis_return_compact_shapes() {
    let root = test_root("projection-apis");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let overview = must_ok(
        storage_hygiene_overview_json(vec![root.display().to_string()], 5, "fast_changed_only"),
        "overview serializes",
    );
    let overview = parse_json_value(&overview, "overview JSON parses");
    assert!(
        overview["items"]
            .as_array()
            .is_some_and(|items| !items.is_empty() && items.len() <= 12)
    );
    assert!(overview.get("cleanup_recipes").is_some());
    assert!(overview.get("cleanup_bundles").is_some());
    assert!(overview.get("summary").is_some());
    assert!(overview.get("diagnostics").is_some());
    assert!(
        overview["treemap_roots"]
            .as_array()
            .is_some_and(|roots| !roots.is_empty())
    );
    assert_eq!(overview["treemap_roots"][0]["node_type"], "root");

    let actions =
        storage_hygiene_actions_json(vec![root.display().to_string()], 5, 80, "fast_changed_only");
    let actions = must_ok(actions, "actions serializes");
    let actions = parse_json_value(&actions, "actions JSON parses");
    assert!(actions.get("items").is_none());
    assert!(actions.get("cleanup_bundles").is_some());

    let page = storage_hygiene_items_page_json(
        vec![root.display().to_string()],
        5,
        0,
        1,
        "fast_changed_only",
        "path",
        false,
    );
    let page = must_ok(page, "items page serializes");
    let page = parse_json_value(&page, "page JSON parses");
    assert_eq!(page["sort_key"], "path");
    assert_eq!(page["sort_descending"], false);
    assert_eq!(page["returned_count"], 1);
    assert!(
        page["items"]
            .as_array()
            .is_some_and(|items| items.len() == 1)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_indexed_snapshot_reuses_persistent_rows() {
    let root = test_root("indexed-snapshot");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let scan = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );
    assert!(scan.contains("\"storage_index_writes\""));

    let overview = must_ok(
        storage_hygiene_overview_json(vec![root.display().to_string()], 5, "instant_cached"),
        "indexed overview serializes",
    );
    let overview = parse_json_value(&overview, "indexed overview JSON parses");
    assert_eq!(overview["scan_mode"], "instant_cached");
    assert_eq!(overview["diagnostics"]["root_walk_millis"], 0);
    assert!(
        overview["items"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        overview["treemap_roots"]
            .as_array()
            .is_some_and(|roots| !roots.is_empty())
    );

    let indexed = must_ok(
        storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80),
        "indexed snapshot serializes",
    );
    let indexed = parse_json_value(&indexed, "indexed JSON parses");

    assert_eq!(indexed["scan_mode"], "instant_cached");
    assert_eq!(indexed["diagnostics"]["root_walk_millis"], 0);
    assert_eq!(indexed["items"][0]["kind"], "rust-build");
    assert!(
        indexed["diagnostics"]["storage_index_hits"]
            .as_u64()
            .is_some_and(|hits| hits >= 1)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_attributes_artifacts_to_git_repo_and_branch() {
    let root = test_root("attributes-artifacts");
    let project = root.join("Aetower");
    let target = project.join("target").join("debug");
    create_git_repo(&project, "master");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        project.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/aetower-ffi\"]\n",
    ) {
        panic!("write cargo manifest: {error}");
    }
    let artifact = target.join("blob");
    if let Err(error) = fs::write(&artifact, vec![0u8; (MIN_ITEM_BYTES + 128) as usize]) {
        panic!("write build artifact: {error}");
    }
    let artifact_metadata = fs::metadata(&artifact).expect("artifact metadata is readable");
    let expected_logical_bytes = artifact_metadata.len();
    let expected_physical_bytes = artifact_metadata.blocks().saturating_mul(512);
    mark_tree_old(&project);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"repo_name\":\"Aetower\""));
    assert!(json.contains("\"git_branch\":\"master\""));
    assert!(json.contains("\"git_head\":\"1234567890ab\""));
    assert!(json.contains("\"attributed_repo_count\":1"));
    assert!(json.contains("\"repo_footprints\""));
    assert!(json.contains("\"repository_inventory\""));
    assert!(json.contains(&format!("\"current_size_bytes\":{expected_physical_bytes}")));
    assert!(json.contains(&format!("\"logical_bytes\":{expected_logical_bytes}")));
    assert!(json.contains(&format!("\"physical_bytes\":{expected_physical_bytes}")));
    assert!(json.contains("\"top_artifact_folders\""));
    assert!(json.contains("\"artifact_mix\""));
    assert!(json.contains("\"label\":\"Cargo target\""));
    assert!(json.contains("\"rebuild_command\":\"cargo build\""));
    assert!(json.contains("\"duplicate_clone_count\":1"));
    assert!(json.contains("\"last_branch_touched\":\"master\""));
    assert!(json.contains("\"estimated_rebuild_cost\":\"Low\""));
    assert!(json.contains("\"last_writer_process\":null"));
    assert!(json.contains("\"cleanup_recipes\""));
    assert!(json.contains("\"title\":\"Clean Rust package aetower-ffi\""));
    assert!(json.contains("cargo clean -p aetower-ffi"));
    assert!(json.contains("\"cleanup_bundles\""));
    assert!(json.contains("\"id\":\"safe-reclaim\""));
    assert!(json.contains("\"dry_run_only\":true"));
    assert!(json.contains("\"confidence_score\":97"));
    assert!(json.contains("\"rollback_notes\""));
    assert!(json.contains("\"budget_guardrails\""));
    assert!(json.contains("\"repo_growth_budget_bytes_per_day\":2147483648"));
    assert!(json.contains("\"total_artifact_budget_bytes\":32212254720"));
    assert!(json.contains("\"free_space_floor_bytes\":21474836480"));
    assert!(json.contains("\"volume_pressure_floor_percent\":10"));
    assert!(json.contains("\"warning_only_by_default\":true"));
    assert!(json.contains("\"auto_trash_safe_tier_enabled\":false"));
    assert!(json.contains("\"scheduled_scan_interval_hours\":24"));
    assert!(json.contains("\"id\":\"safe-tier-auto-trash\""));
    assert!(json.contains("\"enabled\":false"));
    assert!(json.contains("\"prevention_suggestions\""));
    assert!(json.contains("\"action_label\":\"Review scan policy\""));
    let value = parse_json_value(&json, "storage hygiene JSON parses");
    let budget_violations = value["budget_guardrails"]["violations"]
        .as_array()
        .unwrap_or_else(|| panic!("budget guardrail violations serialize as an array"));
    assert!(
        budget_violations
            .iter()
            .all(|violation| violation["scope"] == "volume-pressure")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_repo_footprint_surfaces_duplicate_clone_groups() {
    let root = test_root("repo-footprint-clone-groups");
    let clone_a = root.join("CloneA");
    let clone_b = root.join("CloneB");
    create_git_repo(&clone_a, "main");
    create_git_repo(&clone_b, "main");
    for repo in [&clone_a, &clone_b] {
        if let Err(error) = fs::write(
            repo.join(".git").join("config"),
            "[remote \"origin\"]\n\turl = git@github.com:example/shared.git\n",
        ) {
            panic!("write git remote config: {error}");
        }
    }
    let target = clone_a.join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value = parse_json_value(&json, "storage hygiene clone group JSON parses");
    let footprints = value["repo_footprints"]
        .as_array()
        .unwrap_or_else(|| panic!("repo footprints is an array"));
    let clone_a_root = clone_a.display().to_string();
    let clone_a_footprint = footprints
        .iter()
        .find(|footprint| footprint["repo_root"].as_str() == Some(clone_a_root.as_str()))
        .unwrap_or_else(|| panic!("clone A footprint is present"));

    assert_eq!(clone_a_footprint["duplicate_clone_count"].as_u64(), Some(2));
    assert!(
        clone_a_footprint["duplicate_clone_roots"]
            .as_array()
            .is_some_and(|roots| roots.len() == 2)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_indexes_git_repositories_without_artifacts() {
    let root = test_root("indexes-clean-repositories");
    let repo_a = root.join("CleanOne");
    let repo_b = root.join("Nested").join("CleanTwo");
    create_git_repo(&repo_a, "main");
    create_git_repo(&repo_b, "feature/clean");

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };

    assert_eq!(inventory.len(), 2);
    assert!(inventory.iter().any(|repo| repo["repo_name"] == "CleanOne"));
    assert!(inventory.iter().any(|repo| repo["repo_name"] == "CleanTwo"));
    assert!(
        inventory
            .iter()
            .any(|repo| repo["git_branch"] == "feature/clean")
    );
    assert!(value.get("repo_footprints").is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_reports_agent_guidance_quality() {
    let root = test_root("repository-quality");
    let valid_repo = root.join("ValidGuidance");
    let invalid_repo = root.join("InvalidGuidance");
    let oversized_repo = root.join("OversizedGuidance");
    create_git_repo(&valid_repo, "main");
    create_git_repo(&invalid_repo, "main");
    create_git_repo(&oversized_repo, "main");
    if let Err(error) = fs::write(valid_repo.join("AGENTS.md"), "# Agent guidance\n") {
        panic!("write AGENTS.md: {error}");
    }
    if let Err(error) = fs::write(
        valid_repo.join("CLAUDE.md"),
        "@AGENTS.md\n\n## Claude Code\n\nSupplemental Claude-specific notes.\n",
    ) {
        panic!("write CLAUDE.md: {error}");
    }
    if let Err(error) = fs::write(invalid_repo.join("CLAUDE.md"), "# Claude guidance\n") {
        panic!("write invalid CLAUDE.md: {error}");
    }
    if let Err(error) = fs::write(
        oversized_repo.join("CLAUDE.md"),
        format!(
            "@AGENTS.md\n{}",
            "x".repeat(CLAUDE_MD_DELEGATION_MAX_BYTES as usize)
        ),
    ) {
        panic!("write oversized CLAUDE.md: {error}");
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let valid = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "ValidGuidance")
    {
        Some(repo) => repo,
        None => panic!("valid repo is indexed"),
    };
    let invalid = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "InvalidGuidance")
    {
        Some(repo) => repo,
        None => panic!("invalid repo is indexed"),
    };
    let oversized = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "OversizedGuidance")
    {
        Some(repo) => repo,
        None => panic!("oversized repo is indexed"),
    };

    assert_eq!(valid["has_agents_md"], true);
    assert_eq!(valid["has_claude_md"], true);
    assert_eq!(valid["claude_md_delegates_to_agents_md"], true);
    assert_eq!(
        valid["claude_md_delegation_max_bytes"].as_u64(),
        Some(CLAUDE_MD_DELEGATION_MAX_BYTES)
    );
    assert_eq!(invalid["has_agents_md"], false);
    assert_eq!(invalid["has_claude_md"], true);
    assert_eq!(invalid["claude_md_delegates_to_agents_md"], false);
    assert_eq!(oversized["has_claude_md"], true);
    assert_eq!(oversized["claude_md_delegates_to_agents_md"], false);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_reports_agent_guidance_policy_issues() {
    let root = test_root("agent-guidance-policy");
    let missing_repo = root.join("MissingAgents");
    let untracked_repo = root.join("UntrackedAgents");
    create_git_repo(&missing_repo, "main");
    create_git_repo(&untracked_repo, "main");
    if let Err(error) = fs::write(
        untracked_repo.join("AGENTS.md"),
        "## Scope And Precedence\n\nUse targeted staging.\n",
    ) {
        panic!("write AGENTS.md: {error}");
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let missing = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "MissingAgents")
    {
        Some(repo) => repo,
        None => panic!("missing repo is indexed"),
    };
    let untracked = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "UntrackedAgents")
    {
        Some(repo) => repo,
        None => panic!("untracked repo is indexed"),
    };

    assert_eq!(missing["agent_guidance_status"], "error");
    assert!(guidance_issue_ids(missing).contains(&"agents.root.missing".to_owned()));
    assert_eq!(untracked["agent_guidance_status"], "error");
    assert!(guidance_issue_ids(untracked).contains(&"agents.root.untracked".to_owned()));
    assert!(guidance_issue_ids(untracked).contains(&"agents.sections.missing".to_owned()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_reports_agent_contract_coverage() {
    let root = test_root("agent-contract-coverage");
    let ready_repo = root.join("ReadyRepo");
    let partial_repo = root.join("PartialRepo");
    create_indexed_git_repo(&ready_repo);
    create_indexed_git_repo(&partial_repo);
    write_complete_agent_contracts(&ready_repo);
    write_minimal_agents_contract(&partial_repo);
    git_add_all(&ready_repo);
    git_add_all(&partial_repo);

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let ready = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "ReadyRepo")
    {
        Some(repo) => repo,
        None => panic!("ready repo is indexed"),
    };
    let partial = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "PartialRepo")
    {
        Some(repo) => repo,
        None => panic!("partial repo is indexed"),
    };

    assert_eq!(ready["agent_readiness_status"], "ready");
    assert_eq!(ready["agent_readiness_score"].as_u64(), Some(100));
    assert_eq!(ready["agent_contract_missing_count"].as_u64(), Some(0));
    assert_eq!(
        ready["agent_contract_coverage"].as_array().map(Vec::len),
        Some(10)
    );
    assert_eq!(partial["agent_readiness_status"], "weak");
    assert_eq!(partial["agent_contract_missing_count"].as_u64(), Some(9));
    assert!(guidance_issue_ids(partial).contains(&"agents.contract.missing.repo_map".to_owned()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_pending_human_review_does_not_count_as_reviewed_contract() {
    let root = test_root("pending-human-review-contract");
    let repo = root.join("PendingReviewRepo");
    create_indexed_git_repo(&repo);
    write_complete_agent_contracts(&repo);
    let tasks = repo.join(".agents").join("tasks.yaml");
    if let Err(error) = fs::write(
        &tasks,
        [
            "schema_version: 1",
            "reviewed_by: pending-human-review",
            "reviewed_at: 2026-06-29T00:00:00Z",
            "source_files:",
            "  - AGENTS.md",
            "tasks: []",
            "",
        ]
        .join("\n"),
    ) {
        panic!("write pending review tasks contract: {error}");
    }
    git_add_all(&repo);

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let repository = inventory
        .iter()
        .find(|repo| repo["repo_name"] == "PendingReviewRepo")
        .unwrap_or_else(|| panic!("pending review repo is indexed"));
    let tasks_coverage = repository["agent_contract_coverage"]
        .as_array()
        .and_then(|coverage| coverage.iter().find(|contract| contract["id"] == "tasks"))
        .unwrap_or_else(|| panic!("tasks coverage is present"));

    assert_eq!(tasks_coverage["status"], "partial");
    assert_eq!(tasks_coverage["reviewed"], false);
    assert!(
        tasks_coverage["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("reviewed_by/reviewed_at"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn agent_contract_weights_sum_to_readiness_max() {
    let total: u64 = agent_contract_definitions()
        .iter()
        .map(|definition| definition.weight)
        .sum();
    assert_eq!(total, AGENT_READINESS_MAX_SCORE);
}

#[test]
fn agent_guidance_parser_normalizes_numbered_sections_and_prohibitions() {
    let content = [
        "## 1. Scope And Precedence",
        "",
        "Never use `git add .`, `git add -A`, or `git commit -a`.",
        "",
        "## 2. Repository Map",
        "",
        "## 11. References",
        "",
        "docs/missing.md",
    ]
    .join("\n");
    let headings = markdown_headings(&content)
        .into_iter()
        .map(|heading| heading.title)
        .collect::<Vec<_>>();

    assert_eq!(
        headings,
        vec![
            "Scope And Precedence".to_owned(),
            "Repository Map".to_owned(),
            "References".to_owned()
        ]
    );
    let root = test_root("numbered-references");
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("create numbered references root: {error}");
    }
    let mut issues = Vec::new();
    audit_reference_paths(&mut issues, &root, "AGENTS.md", &content);
    assert!(
        issues
            .iter()
            .any(|issue| issue.id == "agents.references.missing_path")
    );
    let _ = fs::remove_dir_all(root);
    assert!(line_explicitly_prohibits_command(
        "Never use `git add .`, `git add -A`, or `git commit -a`."
    ));
    assert!(!line_explicitly_prohibits_command(
        "Run `git add .` before committing."
    ));
}

#[test]
fn repository_inventory_groups_duplicate_git_remotes() {
    let root = test_root("duplicate-remotes");
    let repo_a = root.join("Mockup");
    let repo_b = root.join("Mockup-Frontend");
    let repo_c = root.join("Aetower");
    create_git_repo(&repo_a, "main");
    create_git_repo(&repo_b, "feature/companion");
    create_git_repo(&repo_c, "main");
    write_git_origin_config(&repo_a, "https://github.com/Aeptus/mockup.git");
    write_git_origin_config(&repo_b, "git@github.com:Aeptus/mockup.git");
    write_git_origin_config(&repo_c, "https://github.com/schiste/Aetower.git");

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let duplicate = match inventory.iter().find(|repo| repo["repo_name"] == "Mockup") {
        Some(repo) => repo,
        None => panic!("duplicate repo is indexed"),
    };
    let unique = match inventory.iter().find(|repo| repo["repo_name"] == "Aetower") {
        Some(repo) => repo,
        None => panic!("unique repo is indexed"),
    };

    assert_eq!(duplicate["git_remote_key"], "github.com/aeptus/mockup");
    assert_eq!(duplicate["git_remote_host"], "github.com");
    assert_eq!(duplicate["git_remote_owner"], "aeptus");
    assert_eq!(duplicate["git_remote_name"], "mockup");
    assert_eq!(duplicate["clone_group_count"], 2);
    assert_eq!(
        duplicate["clone_group_roots"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(unique["clone_group_count"], 1);
    assert_eq!(
        unique["clone_group_roots"].as_array().map(Vec::len),
        Some(0)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_redacts_git_remote_credentials() {
    let root = test_root("remote-redaction");
    let repo = root.join("PrivateRepo");
    create_git_repo(&repo, "main");
    write_git_origin_config(
        &repo,
        "https://user:secret-token@github.com/Aeptus/private.git?access_token=secret-token",
    );

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    assert!(!json.contains("secret-token"));
    assert!(!json.contains("user:"));
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let repo = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "PrivateRepo")
    {
        Some(repo) => repo,
        None => panic!("private repo is indexed"),
    };
    assert_eq!(
        repo["git_remote_origin_url"],
        "https://github.com/Aeptus/private.git"
    );
    assert_eq!(repo["git_remote_key"], "github.com/aeptus/private");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_reads_packed_git_branch_head() {
    let root = test_root("packed-head");
    let repo = root.join("PackedRepo");
    create_git_repo(&repo, "main");
    if let Err(error) = fs::remove_file(repo.join(".git").join("refs").join("heads").join("main")) {
        panic!("remove loose git ref: {error}");
    }
    if let Err(error) = fs::write(
        repo.join(".git").join("packed-refs"),
        "# pack-refs with: peeled fully-peeled sorted\n1234567890abcdef1234567890abcdef12345678 refs/heads/main\n",
    ) {
        panic!("write packed refs: {error}");
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };
    let packed = match inventory
        .iter()
        .find(|repo| repo["repo_name"] == "PackedRepo")
    {
        Some(repo) => repo,
        None => panic!("packed repo is indexed"),
    };

    assert_eq!(packed["git_branch"], "main");
    assert_eq!(packed["git_head"], "1234567890ab");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn local_markdown_paths_ignores_slash_separated_prose() {
    let paths = local_markdown_paths(
        "README.md client/server AI/human ./docs/contracts [Schema](.agents/schema-v1/manifest.schema.json)",
    );

    assert!(paths.contains(&"README.md".to_owned()));
    assert!(paths.contains(&"docs/contracts".to_owned()));
    assert!(paths.contains(&".agents/schema-v1/manifest.schema.json".to_owned()));
    assert!(!paths.contains(&"client/server".to_owned()));
    assert!(!paths.contains(&"AI/human".to_owned()));
}

#[test]
fn repository_inventory_continues_without_artifact_payload_limit() {
    let root = test_root("inventory-ignores-artifact-limit");
    for index in 0..3 {
        let project = root.join(format!("Repo{index}"));
        let target = project.join("target").join("debug");
        create_git_repo(&project, "main");
        if let Err(error) = fs::create_dir_all(&target) {
            panic!("create target dir: {error}");
        }
        if let Err(error) = fs::write(
            target.join("blob"),
            vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write build artifact: {error}");
        }
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("repository inventory JSON parses: {error}"),
    };
    let inventory = match value["repository_inventory"].as_array() {
        Some(inventory) => inventory,
        None => panic!("repository inventory is an array"),
    };

    assert_eq!(inventory.len(), 3);
    assert_eq!(value["repository_inventory_complete"], true);
    assert_eq!(value["repository_inventory_truncated"], false);
    assert_eq!(
        value["repository_inventory_roots"][0].as_str(),
        Some(root.to_str().unwrap_or_default())
    );
    assert!(
        value["repository_inventory_partial_roots"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert!(value.get("items").is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_runs_repository_inventory_before_artifact_scan() {
    let root = test_root("storage-hygiene-inventory-first");
    let repo = root.join("Repo");
    let target = repo.join("target").join("debug");
    let ignored_repo = repo.join("target").join("IgnoredRepo");
    create_git_repo(&repo, "main");
    create_git_repo(&ignored_repo, "main");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("large-artifact"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write artifact file: {error}");
    }
    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 10);
    let value = parse_json_value(&json, "storage hygiene JSON parses");
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let coverage = value["repository_inventory_coverage"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory coverage is an array"));
    let items = value["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items is an array"));
    let names = inventory
        .iter()
        .filter_map(|repo| repo["repo_name"].as_str())
        .collect::<BTreeSet<_>>();

    assert!(names.contains("Repo"));
    assert!(!names.contains("IgnoredRepo"));
    assert_eq!(value["repository_inventory_complete"], true);
    assert_eq!(value["repository_inventory_truncated"], false);
    assert_eq!(coverage[0]["repository_count"].as_u64(), Some(1));
    assert!(!items.is_empty());
    assert!(
        value["summary"]["total_reclaimable_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes >= MIN_ITEM_BYTES)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_json_returns_inventory_without_artifact_payload() {
    let root = test_root("repository-inventory-json");
    let repo = root.join("Repo");
    let ignored_repo = root.join("node_modules").join("IgnoredRepo");

    create_git_repo(&repo, "main");
    create_git_repo(&ignored_repo, "main");
    if let Err(error) = fs::create_dir_all(repo.join("target").join("debug")) {
        panic!("create artifact tree: {error}");
    }
    if let Err(error) = fs::write(
        repo.join("target").join("debug").join("large.o"),
        "artifact",
    ) {
        panic!("write artifact file: {error}");
    }

    let json = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("repository inventory json: {error}"),
    };
    let value = parse_json_value(&json, "repository inventory JSON parses");
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let names = inventory
        .iter()
        .filter_map(|repo| repo["repo_name"].as_str())
        .collect::<BTreeSet<_>>();

    assert!(names.contains("Repo"));
    assert!(!names.contains("IgnoredRepo"));
    assert!(value.get("items").is_none());
    assert!(value.get("repo_footprints").is_none());
    assert_eq!(
        value["repository_inventory_coverage"][0]["repository_count"].as_u64(),
        Some(1)
    );
    assert_eq!(
        value["diagnostics"]["discovered_repository_count"].as_u64(),
        Some(1)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_cache_marks_repos_not_seen_in_latest_scan() {
    let root = test_root("repository-inventory-cache-stale");
    let repo = root.join("CachedRepo");
    create_git_repo(&repo, "main");

    let first = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("initial repository inventory json: {error}"),
    };
    let first_value = parse_json_value(&first, "initial repository inventory JSON parses");
    let first_inventory = first_value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("initial repository inventory is an array"));
    assert_eq!(first_inventory.len(), 1);
    assert_eq!(
        first_inventory[0]["not_seen_in_latest_scan"].as_bool(),
        Some(false)
    );
    assert_eq!(
        first_inventory[0]["inventory_cache_status"].as_str(),
        Some("scanned")
    );
    assert_eq!(
        first_inventory[0]["inventory_fingerprint_changed"].as_bool(),
        Some(false)
    );

    if let Err(error) = fs::remove_dir_all(&repo) {
        panic!("remove repo before stale inventory scan: {error}");
    }

    let second = match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(json) => json,
        Err(error) => panic!("stale repository inventory json: {error}"),
    };
    let second_value = parse_json_value(&second, "stale repository inventory JSON parses");
    let second_inventory = second_value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("stale repository inventory is an array"));

    assert_eq!(second_inventory.len(), 1);
    assert_eq!(second_inventory[0]["repo_name"], "CachedRepo");
    assert_eq!(
        second_inventory[0]["not_seen_in_latest_scan"].as_bool(),
        Some(true)
    );
    assert_eq!(
        second_inventory[0]["inventory_cache_status"].as_str(),
        Some("missing")
    );
    assert_eq!(
        second_inventory[0]["inventory_fingerprint_changed"].as_bool(),
        Some(true)
    );
    assert_eq!(second_value["repository_inventory_complete"], false);
    assert_eq!(
        second_value["diagnostics"]["discovered_repository_count"].as_u64(),
        Some(0)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_indexed_snapshot_returns_cached_repository_inventory_without_artifacts() {
    let root = test_root("indexed-cache-inventory-only");
    let repo = root.join("CachedOnlyRepo");
    create_git_repo(&repo, "main");

    match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(_) => {}
        Err(error) => panic!("prime repository inventory cache: {error}"),
    }

    let json = match storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80) {
        Ok(json) => json,
        Err(error) => panic!("indexed storage hygiene json: {error}"),
    };
    let value = parse_json_value(&json, "indexed cache inventory JSON parses");
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let items = value["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items is an array"));

    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0]["repo_name"], "CachedOnlyRepo");
    assert_eq!(
        inventory[0]["not_seen_in_latest_scan"].as_bool(),
        Some(false)
    );
    assert_eq!(
        inventory[0]["inventory_cache_status"].as_str(),
        Some("current")
    );
    assert_eq!(
        inventory[0]["inventory_fingerprint_changed"].as_bool(),
        Some(false)
    );
    assert!(
        inventory[0]["inventory_fingerprint"]
            .as_str()
            .is_some_and(|fingerprint| fingerprint.starts_with("v1|"))
    );
    assert!(items.is_empty());
    assert_eq!(value["repository_inventory_complete"], false);
    assert_eq!(
        value["repository_inventory_coverage"][0]["status"].as_str(),
        Some("cached_partial")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn repository_inventory_cache_detects_git_metadata_changes() {
    let root = test_root("repository-inventory-cache-fingerprint");
    let repo = root.join("FingerprintRepo");
    create_git_repo(&repo, "main");

    match repository_inventory_json(vec![root.display().to_string()], 5) {
        Ok(_) => {}
        Err(error) => panic!("prime repository inventory cache: {error}"),
    }

    let initial = match storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80) {
        Ok(json) => json,
        Err(error) => panic!("initial indexed storage hygiene json: {error}"),
    };
    let initial_value = parse_json_value(&initial, "initial indexed inventory JSON parses");
    let initial_inventory = initial_value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("initial repository inventory is an array"));
    let initial_fingerprint = initial_inventory[0]["inventory_fingerprint"]
        .as_str()
        .unwrap_or_else(|| panic!("initial inventory fingerprint is a string"))
        .to_owned();
    assert_eq!(
        initial_inventory[0]["inventory_cache_status"].as_str(),
        Some("current")
    );

    if let Err(error) = fs::write(
        repo.join(".git").join("config"),
        "[remote \"origin\"]\n\turl = git@github.com:example/fingerprint.git\n",
    ) {
        panic!("write changed git config: {error}");
    }

    let changed = match storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80) {
        Ok(json) => json,
        Err(error) => panic!("changed indexed storage hygiene json: {error}"),
    };
    let changed_value = parse_json_value(&changed, "changed indexed inventory JSON parses");
    let changed_inventory = changed_value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("changed repository inventory is an array"));
    let changed_fingerprint = changed_inventory[0]["inventory_fingerprint"]
        .as_str()
        .unwrap_or_else(|| panic!("changed inventory fingerprint is a string"));

    assert_eq!(
        changed_inventory[0]["inventory_cache_status"].as_str(),
        Some("changed")
    );
    assert_eq!(
        changed_inventory[0]["inventory_fingerprint_changed"].as_bool(),
        Some(true)
    );
    assert_ne!(initial_fingerprint, changed_fingerprint);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_indexed_snapshot_keeps_artifact_rows_as_repository_overlay_only() {
    let root = test_root("indexed-overlay-only");
    let repo = root.join("OverlayRepo");
    let artifact = repo.join("target").join("debug").join("large.o");
    create_git_repo(&repo, "main");
    if let Some(parent) = artifact.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create artifact directory: {error}");
    }
    if let Err(error) = fs::write(&artifact, vec![0u8; (MIN_ITEM_BYTES + 128) as usize]) {
        panic!("write artifact file: {error}");
    }
    let metadata = match fs::symlink_metadata(&artifact) {
        Ok(metadata) => metadata,
        Err(error) => panic!("artifact metadata: {error}"),
    };
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let storage_index = StorageSizeIndex::open();
    let mut metrics = StorageScanMetrics::default();
    storage_index.store_indexed_row(
        &StorageIndexedFileRow {
            path: artifact.display().to_string(),
            device: metadata.dev() as i64,
            inode: metadata.ino() as i64,
            file_id: "test-overlay-row".to_owned(),
            source_root: root.display().to_string(),
            repo_root: Some(repo.display().to_string()),
            kind: "rust-build".to_owned(),
            storage_role: "build-artifact".to_owned(),
            safety: "safe".to_owned(),
            cleanup_tier: "rebuildable".to_owned(),
            logical_bytes: MIN_ITEM_BYTES + 128,
            physical_bytes: MIN_ITEM_BYTES + 128,
            modified_millis: Some(now_millis),
            changed_millis: Some(now_millis),
            accessed_millis: Some(now_millis),
            birth_millis: Some(now_millis),
            is_directory: false,
            entries: 1,
            truncated: false,
            last_scan_millis: now_millis,
        },
        &mut metrics,
    );

    let json = match storage_hygiene_indexed_json(vec![root.display().to_string()], 5, 80) {
        Ok(json) => json,
        Err(error) => panic!("indexed storage hygiene json: {error}"),
    };
    let value = parse_json_value(&json, "indexed overlay JSON parses");
    let inventory = value["repository_inventory"]
        .as_array()
        .unwrap_or_else(|| panic!("repository inventory is an array"));
    let items = value["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items is an array"));
    let footprints = value["repo_footprints"]
        .as_array()
        .unwrap_or_else(|| panic!("repo footprints is an array"));

    assert!(inventory.is_empty());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["attribution"]["repo_root"].as_str(), repo.to_str());
    assert_eq!(footprints.len(), 1);
    assert_eq!(footprints[0]["repo_root"].as_str(), repo.to_str());
    assert_eq!(
        value["repository_inventory_coverage"][0]["status"].as_str(),
        Some("cached_empty")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_runtime_reports_inventory_before_finalizing() {
    let root = test_root("runtime-repository-inventory-phase");
    let repo = root.join("Repo");
    let target = repo.join("target").join("debug");
    create_git_repo(&repo, "main");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let progress = Arc::new(Mutex::new(StorageScanJobProgress::new(0, None)));
    let runtime = StorageScanRuntimeContext::new(
        Arc::new(StorageScanControl::new()),
        Arc::clone(&progress),
        StorageScanThrottle {
            sleep_every_checkpoints: 0,
            sleep_millis: 0,
            reason: None,
        },
    );
    let report = build_storage_hygiene_report_with_options(
        vec![root.display().to_string()],
        StorageHygieneOptions {
            max_depth: 5,
            limit: 1,
            mode: StorageScanMode::FastChangedOnly,
            runtime: Some(runtime),
            dirty_paths: Vec::new(),
        },
    );
    let progress = lock_or_recover(&progress).clone();

    assert_eq!(progress.phase, STORAGE_SCAN_PHASE_FINALIZING);
    assert!(report.truncated || !report.items.is_empty());
    assert!(report.repository_inventory_complete);
    assert!(!report.repository_inventory_truncated);
    assert!(report.repository_inventory_partial_roots.is_empty());
    assert_eq!(report.repository_inventory.len(), 1);
    assert_eq!(report.repository_inventory_coverage[0].repository_count, 1);
    assert_eq!(report.diagnostics.discovered_repository_count, 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_marks_cancelled_repository_inventory_partial() {
    let root = test_root("runtime-repository-inventory-cancelled");
    let repo = root.join("Repo");
    create_git_repo(&repo, "main");

    let progress = Arc::new(Mutex::new(StorageScanJobProgress::new(0, None)));
    let control = Arc::new(StorageScanControl::new());
    control.cancel();
    let runtime = StorageScanRuntimeContext::new(
        control,
        Arc::clone(&progress),
        StorageScanThrottle {
            sleep_every_checkpoints: 0,
            sleep_millis: 0,
            reason: None,
        },
    );
    let report = build_storage_hygiene_report_with_options(
        vec![root.display().to_string()],
        StorageHygieneOptions {
            max_depth: 5,
            limit: 1,
            mode: StorageScanMode::FastChangedOnly,
            runtime: Some(runtime),
            dirty_paths: Vec::new(),
        },
    );

    assert!(!report.repository_inventory_complete);
    assert!(report.repository_inventory_truncated);
    assert_eq!(
        report.repository_inventory_partial_roots,
        vec![root.display().to_string()]
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_agent_aware_artifact_cost() {
    let root = test_root("agent-aware-artifacts");
    let target = root.join(".codex").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create codex target dir: {error}");
    }
    let artifact = target.join("blob");
    if let Err(error) = fs::write(&artifact, vec![0u8; (MIN_ITEM_BYTES + 128) as usize]) {
        panic!("write codex build artifact: {error}");
    }
    let artifact_metadata = fs::metadata(&artifact).expect("artifact metadata is readable");
    let expected_physical_bytes = artifact_metadata.blocks().saturating_mul(512);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"agent_hygiene\""));
    assert!(json.contains("\"agent_count\":1"));
    assert!(json.contains("\"provider\":\"codex\""));
    assert!(json.contains("\"display_name\":\"Codex\""));
    assert!(json.contains("\"session_id\":\"Codex local artifacts\""));
    assert!(json.contains(&format!(
        "\"week_agent_artifact_bytes\":{expected_physical_bytes}"
    )));
    assert!(json.contains(&format!(
        "\"week_rebuildable_agent_bytes\":{expected_physical_bytes}"
    )));
    assert!(json.contains("\"attribution_sources\":[\"known_agent_directory\"]"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_generates_reclaim_actions_for_common_dev_artifacts() {
    let root = test_root("reclaim-actions");
    let project = root.join("project");
    let node_modules = project.join("node_modules").join("left-pad");
    let venv = project.join(".venv").join("lib");
    let frontend_cache = project.join(".turbo");
    let python_cache = project.join(".pytest_cache");
    let aethyme_cache = project.join(".aethyme").join("agent-contracts").join("v1");
    let coverage = project.join("coverage");
    let next_cache = project.join(".next").join("cache");
    let test_output = project.join("playwright-report");
    let npm_cache = root.join(".npm").join("_cacache");
    let pnpm_store = root
        .join(".local")
        .join("share")
        .join("pnpm")
        .join("store")
        .join("v3");
    let yarn_cache = root.join(".yarn").join("cache");
    let docker_storage = root.join(".docker").join("overlay2");
    let simulator_storage = root
        .join("Library")
        .join("Developer")
        .join("CoreSimulator")
        .join("Devices");
    let xcode_archive = root
        .join("Library")
        .join("Developer")
        .join("Xcode")
        .join("Archives")
        .join("Aetower.xcarchive");
    create_git_repo(&project, "main");

    for directory in [
        &node_modules,
        &venv,
        &frontend_cache,
        &python_cache,
        &aethyme_cache,
        &coverage,
        &next_cache,
        &test_output,
        &npm_cache,
        &pnpm_store,
        &yarn_cache,
        &docker_storage,
        &simulator_storage,
        &xcode_archive,
    ] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create test artifact directory: {error}");
        }
        if let Err(error) = fs::write(
            directory.join("blob"),
            vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write test artifact: {error}");
        }
    }
    mark_tree_old(&root);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"kind\":\"node-dependencies\""));
    assert!(
        json.contains("\"cleanup_consequence\":\"Dependencies can be reinstalled from lockfiles")
    );
    assert!(json.contains("\"rebuild_command\":\"npm install / pnpm install / yarn install\""));
    assert!(json.contains("\"estimated_rebuild_cost\":\"High\""));
    assert!(json.contains("\"title\":\"Remove Node dependency tree\""));
    assert!(json.contains("\"category\":\"node\""));
    assert!(json.contains("\"kind\":\"python-environment\""));
    assert!(json.contains("\"title\":\"Remove Python virtual environment\""));
    assert!(json.contains("\"kind\":\"frontend-cache\""));
    assert!(json.contains("\"title\":\"Clear frontend cache\""));
    assert!(json.contains("\"kind\":\"next-cache\""));
    assert!(json.contains("\"kind\":\"python-cache\""));
    assert!(json.contains("\"title\":\"Clear Python cache\""));
    assert!(json.contains("\"kind\":\"tool-cache\""));
    assert!(json.contains("\"display_name\":\".aethyme\""));
    assert!(json.contains("\"kind\":\"coverage-output\""));
    assert!(json.contains("\"title\":\"Remove coverage output\""));
    assert!(json.contains("\"kind\":\"test-output\""));
    assert!(json.contains("\"title\":\"Remove local test output\""));
    assert!(json.contains("\"kind\":\"npm-cache\""));
    assert!(json.contains("\"kind\":\"pnpm-store\""));
    assert!(json.contains("\"kind\":\"yarn-cache\""));
    assert!(json.contains("\"title\":\"Clear package-manager cache\""));
    assert!(json.contains("\"kind\":\"docker-storage\""));
    assert!(json.contains("\"title\":\"Review Docker storage cleanup\""));
    assert!(json.contains("\"kind\":\"xcode-simulator-runtime\""));
    assert!(json.contains("\"title\":\"Review simulator/device support storage\""));
    assert!(json.contains("\"kind\":\"xcode-archives\""));
    assert!(json.contains("\"optimization_summary\""));
    assert!(json.contains("\"rebuildable_bytes\""));
    assert!(json.contains("rm -rf"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_whole_computer_optimization_buckets() {
    let root = test_root("whole-computer-buckets");
    let downloads = root.join("Downloads");
    let documents = root.join("Documents");
    let archive = root.join("Archive");
    let app_bundle = root
        .join("Applications")
        .join("Sample.app")
        .join("Contents");
    let app_cache = root
        .join("Library")
        .join("Caches")
        .join("com.example.sample");
    let app_container = root
        .join("Library")
        .join("Containers")
        .join("com.example.sample");
    let app_support = root
        .join("Library")
        .join("Application Support")
        .join("Sample");
    let app_preferences = root.join("Library").join("Preferences");
    let app_receipts = root.join("private").join("var").join("db").join("receipts");
    let app_launch_agents = root.join("Library").join("LaunchAgents");
    let ios_backup = root
        .join("Library")
        .join("Application Support")
        .join("MobileSync")
        .join("Backup")
        .join("device-a");
    let mail_attachments = root
        .join("Library")
        .join("Mail")
        .join("V10")
        .join("Attachments")
        .join("thread-a");
    let message_attachments = root
        .join("Library")
        .join("Messages")
        .join("Attachments")
        .join("chat-a");
    let local_snapshot = root.join(".MobileBackups").join("snapshot-a");

    for directory in [
        &downloads,
        &documents,
        &archive,
        &app_bundle,
        &app_cache,
        &app_container,
        &app_support,
        &app_preferences,
        &app_receipts,
        &app_launch_agents,
        &ios_backup,
        &mail_attachments,
        &message_attachments,
        &local_snapshot,
    ] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create whole-computer fixture directory: {error}");
        }
    }

    for path in [
        downloads.join("duplicate-a.pkg"),
        documents.join("duplicate-b.pkg"),
    ] {
        if let Err(error) = fs::write(path, vec![7u8; (MIN_ITEM_BYTES + 512) as usize]) {
            panic!("write duplicate candidate: {error}");
        }
    }
    let large_file = downloads.join("large-video.mov");
    let file = match fs::File::create(&large_file) {
        Ok(file) => file,
        Err(error) => panic!("create large file: {error}"),
    };
    if let Err(error) = file.set_len(LARGE_FILE_BYTES + MIN_ITEM_BYTES) {
        panic!("size large file: {error}");
    }
    if let Err(error) = fs::write(
        archive.join("cold-data.bin"),
        vec![1u8; (MIN_ITEM_BYTES + 256) as usize],
    ) {
        panic!("write cold file: {error}");
    }
    if let Err(error) = fs::write(
        app_bundle.join("Info.plist"),
        "<plist><dict><key>CFBundleIdentifier</key><string>com.example.sample</string></dict></plist>",
    ) {
        panic!("write app Info.plist: {error}");
    }
    if let Err(error) = fs::write(
        app_bundle.join("blob"),
        vec![2u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write app payload: {error}");
    }
    if let Err(error) = fs::write(
        app_preferences.join("com.example.sample.plist"),
        "<plist><dict><key>Enabled</key><true/></dict></plist>",
    ) {
        panic!("write app preference plist: {error}");
    }
    if let Err(error) = fs::write(app_receipts.join("com.example.sample.bom"), "receipt") {
        panic!("write app receipt bom: {error}");
    }
    if let Err(error) = fs::write(app_receipts.join("com.example.sample.plist"), "receipt") {
        panic!("write app receipt plist: {error}");
    }
    if let Err(error) = fs::write(
        app_launch_agents.join("com.example.sample.plist"),
        "<plist><dict><key>Label</key><string>com.example.sample</string></dict></plist>",
    ) {
        panic!("write app launch agent: {error}");
    }
    for directory in [
        &app_cache,
        &app_container,
        &app_support,
        &ios_backup,
        &mail_attachments,
        &message_attachments,
        &local_snapshot,
    ] {
        if let Err(error) = fs::write(
            directory.join("payload"),
            vec![3u8; (MIN_ITEM_BYTES + 128) as usize],
        ) {
            panic!("write whole-computer payload: {error}");
        }
    }
    mark_tree_old(&root);
    match Command::new("touch").arg(&large_file).status() {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("refresh large-file timestamp failed: {status}"),
        Err(error) => panic!("run touch for large-file timestamp: {error}"),
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 8, 120);

    assert!(json.contains("\"kind\":\"large-file\""));
    assert!(json.contains("\"kind\":\"cold-file\""));
    assert!(json.contains("\"duplicate_groups\""));
    assert!(json.contains("\"confirmed\":true"));
    assert!(json.contains("\"reclaimable_bytes\""));
    assert!(json.contains("\"app_footprints\""));
    assert!(json.contains("\"app_name\":\"Sample\""));
    assert!(json.contains("\"bundle_identifier\":\"com.example.sample\""));
    assert!(json.contains("\"kind\":\"app-cache\""));
    assert!(json.contains("\"kind\":\"app-container\""));
    assert!(json.contains("\"kind\":\"app-support-data\""));
    assert!(json.contains("\"kind\":\"app-preferences\""));
    assert!(json.contains("\"kind\":\"app-receipt\""));
    assert!(json.contains("\"kind\":\"app-launch-item\""));
    assert!(json.contains("\"component\":\"Preferences\""));
    assert!(json.contains("\"component\":\"Receipt\""));
    assert!(json.contains("\"component\":\"Launch item\""));
    assert!(json.contains("\"system_data_buckets\""));
    assert!(json.contains("\"kind\":\"ios-backup\""));
    assert!(json.contains("\"kind\":\"mail-attachments\""));
    assert!(json.contains("\"kind\":\"message-attachments\""));
    assert!(json.contains("\"kind\":\"local-snapshot\""));
    assert!(json.contains("\"title\":\"iOS and iPadOS backups\""));
    assert!(json.contains("\"title\":\"Mail attachments\""));
    assert!(json.contains("\"title\":\"Messages attachments\""));
    assert!(json.contains("\"title\":\"Local snapshots\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_resilience_fixture_handles_edge_cases() {
    let root = test_root("resilience-fixtures");
    let npm_cache = root.join(".npm").join("_cacache").join("content-v2");
    let pnpm_store = root.join(".local").join("share").join("pnpm").join("store");
    let logs = root.join("Library").join("Logs").join("Aetower");
    let cloud_root = root
        .join("Library")
        .join("CloudStorage")
        .join("iCloud Drive");
    let denied = root.join("permission-denied");
    let symlink_target = root.join("target").join("debug");
    let symlink_path = root.join("target-link");
    for directory in [
        &npm_cache,
        &pnpm_store,
        &logs,
        &cloud_root,
        &denied,
        &symlink_target,
    ] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create resilience fixture directory: {error}");
        }
    }
    if let Err(error) = fs::write(
        npm_cache.join("npm-cache.bin"),
        vec![1u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write npm cache fixture: {error}");
    }
    if let Err(error) = fs::write(
        pnpm_store.join("pnpm-store.bin"),
        vec![2u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write pnpm store fixture: {error}");
    }
    if let Err(error) = fs::write(
        logs.join("huge.log"),
        vec![3u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write huge log fixture: {error}");
    }
    let sparse_cloud_file = cloud_root.join("dehydrated-placeholder.bin");
    let file = match fs::File::create(&sparse_cloud_file) {
        Ok(file) => file,
        Err(error) => panic!("create sparse cloud placeholder fixture: {error}"),
    };
    if let Err(error) = file.set_len(MIN_ITEM_BYTES + 512) {
        panic!("size sparse cloud placeholder fixture: {error}");
    }
    if let Err(error) = fs::write(
        symlink_target.join("linked-target.bin"),
        vec![4u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write symlink target fixture: {error}");
    }
    if let Err(error) = std::os::unix::fs::symlink(&symlink_target, &symlink_path)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        panic!("create symlink fixture: {error}");
    }
    if let Err(error) = fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)) {
        panic!("restrict permission fixture: {error}");
    }

    let json = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string(), cloud_root.display().to_string()],
        8,
        120,
        "forensic_verified",
    );
    let value = parse_json_value(&json, "resilience fixture JSON parses");
    let item_kinds = value["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items array exists"))
        .iter()
        .filter_map(|item| item["kind"].as_str())
        .collect::<BTreeSet<_>>();

    assert!(item_kinds.contains("npm-cache"));
    assert!(item_kinds.contains("pnpm-store"));
    assert!(item_kinds.contains("log-file"));
    assert!(
        value["source_coverage"]
            .as_array()
            .unwrap_or_else(|| panic!("source coverage array exists"))
            .iter()
            .any(|source| source["cloud_placeholder"].as_bool() == Some(true)
                || source["kind"].as_str() == Some("cloud"))
    );
    assert!(
        value["items"]
            .as_array()
            .unwrap_or_else(|| panic!("items array exists"))
            .iter()
            .all(|item| item["path"]
                .as_str()
                .is_none_or(|path| !path.contains("target-link")))
    );
    assert!(
        value["diagnostics"]["performance_budget"]["status"]
            .as_str()
            .is_some_and(|status| matches!(status, "ok" | "warn" | "critical"))
    );

    let _ = fs::set_permissions(&denied, fs::Permissions::from_mode(0o755));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reclaimable_regression_detects_build_logs_and_caches() {
    let root = test_root("reclaimable-regression");
    let target = root.join("project").join("target").join("debug");
    let logs = root.join("project").join("logs");
    for directory in [&target, &logs] {
        if let Err(error) = fs::create_dir_all(directory) {
            panic!("create reclaimable regression fixture directory: {error}");
        }
    }
    if let Err(error) = fs::write(
        target.join("artifact"),
        vec![1u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact fixture: {error}");
    }
    if let Err(error) = fs::write(
        logs.join("runtime.log"),
        vec![2u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write log fixture: {error}");
    }
    mark_tree_old(&root);

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value = parse_json_value(&json, "reclaimable regression JSON parses");

    assert!(
        value["summary"]["total_reclaimable_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes >= MIN_ITEM_BYTES)
    );
    assert!(
        value["cleanup_bundles"]
            .as_array()
            .is_some_and(|bundles| !bundles.is_empty())
    );
    assert!(
        value["diagnostics"]["performance_budget"]["payload_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_performance_budget_flags_million_file_payload_and_table_pressure() {
    let root = test_root("million-file-budget-fixture");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create budget fixture target: {error}");
    }
    if let Err(error) = fs::write(
        target.join("artifact"),
        vec![1u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write budget fixture artifact: {error}");
    }
    let mut report = build_storage_hygiene_report_with_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );
    report.scan_duration_millis = STORAGE_SCAN_LATENCY_CRITICAL_MILLIS;
    report.diagnostics.payload_bytes = STORAGE_PAYLOAD_CRITICAL_BYTES;
    report.diagnostics.candidate_seen_count = 1_000_000;

    refresh_storage_performance_budget(
        &mut report,
        STORAGE_TABLE_PAGE_CRITICAL_MILLIS,
        STORAGE_RENDER_CRITICAL_MILLIS,
    );

    assert_eq!(report.diagnostics.performance_budget.status, "critical");
    assert_eq!(
        report
            .diagnostics
            .performance_budget
            .scan_job_latency_millis,
        STORAGE_SCAN_LATENCY_CRITICAL_MILLIS
    );
    assert_eq!(
        report.diagnostics.performance_budget.table_page_millis,
        STORAGE_TABLE_PAGE_CRITICAL_MILLIS
    );
    assert_eq!(
        report.diagnostics.performance_budget.render_publish_millis,
        STORAGE_RENDER_CRITICAL_MILLIS
    );
    assert!(
        report
            .diagnostics
            .performance_budget
            .notes
            .iter()
            .any(|note| note.contains("1000000 candidates seen"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn protected_path_classification_blocks_unattended_cleanup() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let mut items = vec![test_storage_item(
        "/Applications/Important.app/Contents/MacOS/cache.bin",
        "app-cache",
        "cache",
        "safe",
        "safe",
        old_millis,
    )];

    apply_cleanup_guardrails(&mut items, now_millis);

    assert!(items[0].protected_path);
    assert_eq!(items[0].cleanup_tier, "risky");
    assert_eq!(items[0].default_cleanup_action, "manual_review");
    assert!(!items[0].cleanup_allowed);
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|blocker| blocker.contains("Protected system/application path"))
    );
}

#[test]
fn storage_growth_attribution_uses_single_writer_record() {
    let root = test_root("growth-attribution-single-writer");
    let repo = root.join("project");
    create_git_repo(&repo, "main");
    let artifact_path = repo.join("target").join("debug");
    let delta = test_growth_delta(&artifact_path, Some(&repo), 1_000, 128, 4_096);
    let writer = StorageWriterLedgerRecord {
        started_at_millis: Some(900),
        ended_at_millis: Some(1_100),
        path_prefix: Some(repo.display().to_string()),
        repo_root: Some(repo.display().to_string()),
        command: Some("cargo build".to_owned()),
        process_tree: Some("zsh > cargo build".to_owned()),
        ai_agent_session: Some("Claude session A".to_owned()),
        source: Some("test-ledger".to_owned()),
        ..StorageWriterLedgerRecord::default()
    };

    let attribution = attribute_storage_growth_delta(&delta, &[writer]);

    assert_eq!(attribution.confidence, "high");
    assert_eq!(attribution.confidence_score, 92);
    assert!(!attribution.ambiguous);
    assert_eq!(attribution.writer_source.as_deref(), Some("test-ledger"));
    assert_eq!(attribution.matched_writer_count, 1);
    assert!(attribution.sources.contains(&"writer_ledger".to_owned()));
    assert!(attribution.sources.contains(&"command".to_owned()));
    assert!(attribution.sources.contains(&"process_tree".to_owned()));
    assert!(attribution.sources.contains(&"ai_session".to_owned()));
    assert_eq!(attribution.command.as_deref(), Some("cargo build"));
    assert_eq!(
        attribution.ai_agent_session.as_deref(),
        Some("Claude session A")
    );
    assert_eq!(attribution.git_branch.as_deref(), Some("main"));
    assert!(
        attribution
            .evidence
            .iter()
            .any(|entry| entry.contains("Writer ledger source"))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_attribution_marks_overlapping_writers_ambiguous() {
    let root = test_root("growth-attribution-ambiguous");
    let repo = root.join("project");
    create_git_repo(&repo, "main");
    let artifact_path = repo.join(".build");
    let delta = test_growth_delta(&artifact_path, Some(&repo), 2_000, 256, 8_192);
    let writers = vec![
        StorageWriterLedgerRecord {
            started_at_millis: Some(1_900),
            ended_at_millis: Some(2_100),
            path_prefix: Some(repo.display().to_string()),
            command: Some("swift build".to_owned()),
            ..StorageWriterLedgerRecord::default()
        },
        StorageWriterLedgerRecord {
            started_at_millis: Some(1_950),
            ended_at_millis: Some(2_050),
            path_prefix: Some(repo.display().to_string()),
            ai_agent_session: Some("Codex session B".to_owned()),
            ..StorageWriterLedgerRecord::default()
        },
    ];

    let attribution = attribute_storage_growth_delta(&delta, &writers);

    assert_eq!(attribution.confidence, "ambiguous");
    assert!(attribution.ambiguous);
    assert!(attribution.command.is_none());
    assert_eq!(attribution.matched_writer_count, 2);
    assert!(attribution.sources.contains(&"writer_ledger".to_owned()));
    assert!(
        attribution
            .sources
            .contains(&"ambiguous_writers".to_owned())
    );
    assert!(
        attribution
            .summary
            .contains("Multiple writer records overlapped")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_growth_attribution_reports_controlled_build_from_chau7_writer() {
    let root = test_root("growth-attribution-chau7-controlled-build");
    let repo = root.join("project");
    create_git_repo(&repo, "feature/storage-attribution");
    let artifact_path = repo.join("target").join("release").join("aetower");
    let delta = test_growth_delta(&artifact_path, Some(&repo), 1_000, 0, 32 * 1024 * 1024);
    let writer = StorageWriterLedgerRecord {
        started_at_millis: Some(900),
        ended_at_millis: Some(1_100),
        working_directory: Some(repo.display().to_string()),
        provider: Some("claude".to_owned()),
        session_id: Some("chau7-tab-a".to_owned()),
        source: Some("chau7".to_owned()),
        command: Some("cargo build --workspace --release".to_owned()),
        process_tree: Some("Chau7 > zsh > cargo build --workspace --release".to_owned()),
        ..StorageWriterLedgerRecord::default()
    };

    let attribution = attribute_storage_growth_delta(&delta, &[writer]);

    assert_eq!(attribution.confidence, "high");
    assert_eq!(attribution.confidence_score, 92);
    assert!(!attribution.ambiguous);
    assert_eq!(attribution.writer_source.as_deref(), Some("chau7"));
    assert_eq!(attribution.matched_writer_count, 1);
    assert_eq!(
        attribution.command.as_deref(),
        Some("cargo build --workspace --release")
    );
    assert_eq!(
        attribution.process_tree.as_deref(),
        Some("Chau7 > zsh > cargo build --workspace --release")
    );
    assert_eq!(
        attribution.ai_agent_session.as_deref(),
        Some("claude session chau7-tab-a")
    );
    assert_eq!(
        attribution.git_branch.as_deref(),
        Some("feature/storage-attribution")
    );
    assert!(attribution.sources.contains(&"writer_ledger".to_owned()));
    assert!(attribution.sources.contains(&"chau7".to_owned()));
    assert!(attribution.sources.contains(&"command".to_owned()));
    assert!(attribution.sources.contains(&"process_tree".to_owned()));
    assert!(attribution.sources.contains(&"ai_session".to_owned()));
    assert!(
        attribution
            .summary
            .contains("Single writer ledger record matched")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn cleanup_guardrails_block_untracked_source_like_files() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let mut items = vec![test_storage_item(
        "/tmp/aetower-storage/source/main.swift",
        "large-file",
        "large-file",
        "safe",
        "safe",
        now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000),
    )];
    items[0].git_status = "untracked".to_owned();

    apply_cleanup_guardrails(&mut items, now_millis);

    assert!(!items[0].cleanup_allowed);
    assert_eq!(items[0].cleanup_tier, "risky");
    assert_eq!(items[0].safety, "review");
    assert_eq!(items[0].default_cleanup_action, "manual_review");
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|reason| reason.contains("Untracked source-like file"))
    );
    assert!(build_cleanup_recipes(&items).is_empty());
    assert!(build_cleanup_bundles(&items).is_empty());
}

#[test]
fn cleanup_guardrails_block_tracked_modified_and_protected_paths() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let mut items = vec![
        test_storage_item(
            "/tmp/aetower-storage/project/target/debug/blob",
            "rust-build",
            "build-artifact",
            "safe",
            "rebuildable",
            old_millis,
        ),
        test_storage_item(
            "/Applications/Aetower.app",
            "tool-cache",
            "cache",
            "safe",
            "safe",
            old_millis,
        ),
    ];
    items[0].git_status = "modified".to_owned();

    apply_cleanup_guardrails(&mut items, now_millis);

    assert!(!items[0].cleanup_allowed);
    assert!(
        items[0]
            .cleanup_blockers
            .iter()
            .any(|reason| reason.contains("source work is protected"))
    );
    assert!(!items[1].cleanup_allowed);
    assert!(
        items[1]
            .cleanup_blockers
            .iter()
            .any(|reason| reason.contains("Protected system/application path"))
    );
}

#[test]
fn cleanup_recipes_require_trash_actionable_items() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let allowed = test_storage_item(
        "/tmp/aetower-storage/logs/aetower.log",
        "log-file",
        "log",
        "safe",
        "safe",
        old_millis,
    );
    let mut blocked = allowed.clone();
    blocked.path = "/tmp/aetower-storage/logs/recent.log".to_owned();
    blocked.id = blocked.path.clone();
    blocked.modified_millis = Some(now_millis);

    let mut items = vec![allowed.clone(), blocked];
    apply_cleanup_guardrails(&mut items, now_millis);

    let recipes = build_cleanup_recipes(&items);
    assert!(items[0].cleanup_allowed);
    assert!(!items[1].cleanup_allowed);
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].affected_path, allowed.path);
}

#[test]
fn cleanup_bundle_manifest_carries_policy_and_excludes_blocked_items() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let mut allowed = test_storage_item(
        "/tmp/aetower-storage/project/.build/debug/cache.bin",
        "swift-build",
        "build-artifact",
        "safe",
        "rebuildable",
        old_millis,
    );
    allowed.evidence = vec![
        "Detected under a SwiftPM .build directory.".to_owned(),
        "Source-control status: outside Git.".to_owned(),
    ];
    let mut blocked = test_storage_item(
        "/tmp/aetower-storage/project/src/main.swift",
        "large-file",
        "large-file",
        "safe",
        "safe",
        old_millis,
    );
    blocked.git_status = "tracked".to_owned();

    let mut items = vec![allowed.clone(), blocked];
    apply_cleanup_guardrails(&mut items, now_millis);

    let bundles = build_cleanup_bundles(&items);
    let manifest_item = bundles
        .iter()
        .flat_map(|bundle| bundle.manifest.iter())
        .find(|item| item.path == allowed.path)
        .expect("allowed item is present in cleanup manifest");

    assert_eq!(manifest_item.path, allowed.path);
    assert_eq!(manifest_item.size_bytes, allowed.size_bytes);
    assert_eq!(manifest_item.cleanup_tier, "rebuildable");
    assert_eq!(manifest_item.default_cleanup_action, "trash");
    assert!(manifest_item.cleanup_allowed);
    assert!(manifest_item.cleanup_blockers.is_empty());
    assert_eq!(manifest_item.reason, "test item");
    assert_eq!(manifest_item.consequence, "test cleanup consequence");
    assert!(
        manifest_item
            .evidence
            .iter()
            .any(|entry| entry.contains("SwiftPM"))
    );
    assert!(
        bundles
            .iter()
            .flat_map(|bundle| bundle.manifest.iter())
            .all(|item| !item.path.ends_with("src/main.swift"))
    );
}

#[test]
fn storage_release_criteria_dry_run_manifest_validates_bytes_paths_and_blocks_risky() {
    let now_millis = crate::current_unix_millis().unwrap_or_default();
    let old_millis = now_millis.saturating_sub(RECENT_CLEANUP_BLOCK_MILLIS + 60_000);
    let allowed = test_storage_item(
        "/tmp/aetower-storage/project/target/debug/cache.bin",
        "rust-build",
        "build-artifact",
        "safe",
        "rebuildable",
        old_millis,
    );
    let mut source_like = test_storage_item(
        "/tmp/aetower-storage/project/src/generated.swift",
        "large-file",
        "large-file",
        "safe",
        "safe",
        old_millis,
    );
    source_like.git_status = "untracked".to_owned();

    let mut items = vec![allowed.clone(), source_like];
    apply_cleanup_guardrails(&mut items, now_millis);

    let bundles = build_cleanup_bundles(&items);
    let bundle = bundles
        .iter()
        .find(|bundle| bundle.manifest.iter().any(|item| item.path == allowed.path))
        .expect("dry-run bundle includes only the release-safe artifact");
    let manifest_bytes = bundle
        .manifest
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.size_bytes));

    assert!(bundle.dry_run_only);
    assert_eq!(bundle.estimated_reclaimable_bytes, manifest_bytes);
    assert_eq!(bundle.item_count, bundle.manifest.len());
    assert!(bundle.estimated_reclaimable_bytes > 0);
    assert!(bundle.manifest.iter().all(|item| !item.path.is_empty()));
    assert!(bundle.manifest.iter().all(|item| item.cleanup_allowed));
    assert!(
        bundle
            .manifest
            .iter()
            .all(|item| item.cleanup_blockers.is_empty())
    );
    assert!(
        bundle
            .manifest
            .iter()
            .all(|item| item.default_cleanup_action == "trash")
    );
    assert!(
        bundle
            .dry_run_commands
            .iter()
            .all(|command| command.starts_with("du -sh "))
    );
    assert!(
        bundles
            .iter()
            .flat_map(|bundle| bundle.manifest.iter())
            .all(|item| !item.path.ends_with("src/generated.swift"))
    );
    assert!(!items[1].cleanup_allowed);
    assert_eq!(items[1].default_cleanup_action, "manual_review");
}

#[test]
fn storage_release_criteria_scan_cancel_responds_under_one_second() {
    let root = test_root("release-cancel-budget");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create release cancel fixture: {error}");
    }
    if let Err(error) = fs::write(
        target.join("artifact"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write release cancel artifact: {error}");
    }

    let start = must_ok(
        storage_scan_start_json(
            vec![root.display().to_string()],
            6,
            120,
            "deep_native",
            "battery,thermal-pressure",
            Vec::new(),
        ),
        "start release cancel scan job",
    );
    let start = parse_json_value(&start, "decode release cancel start");
    let job_id = json_string(&start, "job_id").to_owned();

    let cancel_started = Instant::now();
    let cancel = must_ok(storage_scan_cancel_json(&job_id), "cancel release scan job");
    let elapsed = cancel_started.elapsed();
    let cancel = parse_json_value(&cancel, "decode release cancel response");

    assert!(
        elapsed < Duration::from_secs(1),
        "cancel took {:?}, expected under one second",
        elapsed
    );
    assert_eq!(cancel["status"].as_str(), Some("cancelled"));
    assert!(storage_scan_result_json(&job_id).is_err());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_investigation_evidence_and_large_files() {
    let root = test_root("investigation-evidence");
    let project = root.join("LargeRepo");
    create_git_repo(&project, "main");
    if let Err(error) = fs::write(project.join(".gitignore"), "ignored.bin\n") {
        panic!("write gitignore: {error}");
    }
    let large_file = project.join("ignored.bin");
    let file = match fs::File::create(&large_file) {
        Ok(file) => file,
        Err(error) => panic!("create large file: {error}"),
    };
    if let Err(error) = file.set_len(LARGE_FILE_BYTES + MIN_ITEM_BYTES) {
        panic!("size large file: {error}");
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("storage hygiene JSON parses: {error}"),
    };
    let items = match value["items"].as_array() {
        Some(items) => items,
        None => panic!("items is an array"),
    };
    let large = match items.iter().find(|item| item["kind"] == "large-file") {
        Some(item) => item,
        None => panic!("large file is detected"),
    };

    assert_eq!(value["investigation"]["large_file_count"].as_u64(), Some(1));
    assert!(
        value["investigation"]["top_findings"]
            .as_array()
            .is_some_and(|findings| findings
                .iter()
                .any(|finding| finding["path"] == large_file.display().to_string()))
    );
    assert_eq!(large["storage_role"], "large-file");
    assert_eq!(large["git_status"], "ignored");
    assert!(
        large["next_step"]
            .as_str()
            .is_some_and(|step| step.contains("Manual review"))
    );
    assert!(
        large["evidence"]
            .as_array()
            .is_some_and(|evidence| evidence.len() >= 4)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_cold_unaccessed_files() {
    let root = test_root("cold-files");
    let project = root.join("ColdRepo");
    create_git_repo(&project, "main");
    let cold_file = project.join("cold-data.bin");
    let file = match fs::File::create(&cold_file) {
        Ok(file) => file,
        Err(error) => panic!("create cold file: {error}"),
    };
    if let Err(error) = file.set_len(MIN_ITEM_BYTES + 256) {
        panic!("size cold file: {error}");
    }
    match Command::new("touch")
        .arg("-a")
        .arg("-t")
        .arg("202001010000")
        .arg(&cold_file)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("set old access time failed: {status}"),
        Err(error) => panic!("run touch for old access time: {error}"),
    }

    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(error) => panic!("storage hygiene JSON parses: {error}"),
    };
    let items = match value["items"].as_array() {
        Some(items) => items,
        None => panic!("items is an array"),
    };
    let cold = match items.iter().find(|item| item["kind"] == "cold-file") {
        Some(item) => item,
        None => panic!("cold file is detected"),
    };

    assert_eq!(value["investigation"]["cold_file_count"].as_u64(), Some(1));
    assert_eq!(cold["cold"], true);
    assert!(
        cold["access_age_days"]
            .as_u64()
            .is_some_and(|days| days >= COLD_AFTER_DAYS)
    );
    assert!(
        cold["evidence"]
            .as_array()
            .is_some_and(|evidence| evidence.iter().any(|entry| entry
                .as_str()
                .is_some_and(|text| text.contains("cold-file threshold"))))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_hygiene_reports_missing_roots_without_failing() {
    let root = test_root("missing-root");
    let json = build_storage_hygiene_report_for_roots(vec![root.display().to_string()], 5, 80);

    assert!(json.contains("\"skipped_roots\""));
    assert!(json.contains("\"reason\":\"missing\""));
}

#[test]
fn storage_hygiene_reports_source_and_volume_coverage() {
    let root = test_root("source-coverage");
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("create root: {error}");
    }
    let json = build_storage_hygiene_report_for_roots_mode(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
    );

    assert!(json.contains("\"source_coverage\""));
    assert!(json.contains("\"volume_states\""));
    assert!(json.contains("\"permission_state\":\"readable\""));
    assert!(json.contains("\"gap_kind\":\"covered\""));
    assert!(json.contains("\"protected\":false"));
    assert!(json.contains("\"free_now_bytes\""));
}

#[test]
fn storage_scan_job_completes_and_returns_result() {
    let root = test_root("scan-job-completes");
    let target = root.join("project").join("target").join("debug");
    if let Err(error) = fs::create_dir_all(&target) {
        panic!("create target dir: {error}");
    }
    if let Err(error) = fs::write(
        target.join("blob"),
        vec![0u8; (MIN_ITEM_BYTES + 128) as usize],
    ) {
        panic!("write build artifact: {error}");
    }

    let start = must_ok(
        storage_scan_start_json(
            vec![root.display().to_string()],
            5,
            80,
            "fast_changed_only",
            "normal",
            Vec::new(),
        ),
        "start scan job",
    );
    let start = parse_json_value(&start, "decode start");
    let job_id = json_string(&start, "job_id").to_owned();
    assert_eq!(start["resumed_from_partial"], false);
    assert_eq!(start["partial_state_available"], true);
    assert!(start["persisted_at_millis"].as_u64().is_some());

    let mut terminal_status = String::new();
    for _ in 0..80 {
        let status = must_ok(storage_scan_status_json(&job_id), "status scan job");
        let status = parse_json_value(&status, "decode status");
        terminal_status = status["status"].as_str().unwrap_or_default().to_owned();
        if terminal_status == "complete" || terminal_status == "failed" {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    assert_eq!(terminal_status, "complete");
    assert_eq!(
        StorageScanStateStore::load_status_for_job(&job_id).as_deref(),
        Some("complete")
    );
    let result = must_ok(storage_scan_result_json(&job_id), "scan result");
    assert!(result.contains("\"scan_mode\":\"fast_changed_only\""));
    assert!(result.contains("\"repository_inventory_coverage\""));
    assert!(result.contains("\"rust-build\""));
}

#[test]
fn storage_scan_job_recovers_persisted_partial_state() {
    let root = test_root("scan-job-recovers-partial");
    if let Err(error) = fs::create_dir_all(&root) {
        panic!("create root: {error}");
    }
    let request = StorageScanJobRequest::new(
        vec![root.display().to_string()],
        5,
        80,
        "fast_changed_only",
        "battery",
        Vec::new(),
    );
    let mut progress =
        StorageScanJobProgress::new(storage_now_millis(), Some("battery".to_owned()));
    progress.phase = "artifact_sizing".to_owned();
    progress.scanned_files = 17;
    progress.scanned_directories = 5;
    progress.scanned_bytes = 42_000;
    must_ok(
        StorageScanStateStore::persist(StorageScanPersistedRecord {
            job_id: format!("persisted-partial-{}", storage_now_millis()),
            signature: request.signature.clone(),
            volume_key: request.volume_key.clone(),
            roots: request.normalized_roots.clone(),
            dirty_paths: request.dirty_paths.clone(),
            max_depth: request.max_depth,
            limit: request.limit,
            mode: request.mode.as_str().to_owned(),
            throttle_hint: request.throttle_hint.clone(),
            status: "running".to_owned(),
            progress,
            started_at_millis: storage_now_millis().saturating_sub(1_000),
            updated_at_millis: storage_now_millis(),
            completed_at_millis: None,
            result_available: false,
            resume_available: true,
        }),
        "persist partial scan state",
    );

    let start = must_ok(
        storage_scan_start_json(
            vec![root.display().to_string()],
            5,
            80,
            "fast_changed_only",
            "battery",
            Vec::new(),
        ),
        "start resumed scan job",
    );
    let start = parse_json_value(&start, "decode resumed start");
    let job_id = json_string(&start, "job_id").to_owned();

    assert_eq!(start["resumed_from_partial"], true);
    assert_eq!(start["partial_state_available"], true);
    assert_eq!(start["recovered_files"].as_u64(), Some(17));
    assert_eq!(start["recovered_directories"].as_u64(), Some(5));
    assert_eq!(start["recovered_bytes"].as_u64(), Some(42_000));
    assert!(
        start["progress"]["throttle_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("battery"))
    );

    let _ = storage_scan_cancel_json(&job_id);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_scan_throttle_detects_pressure_cloud_and_network_roots() {
    let request = StorageScanJobRequest::new(
        vec![
            "/Volumes/SharedBuilds".to_owned(),
            "~/Library/CloudStorage/Dropbox".to_owned(),
        ],
        5,
        80,
        "deep_native",
        "battery,thermal-pressure,network",
        Vec::new(),
    );
    let throttle = StorageScanThrottle::for_request(&request);
    let reason = throttle.reason.unwrap_or_default();

    assert!(throttle.sleep_every_checkpoints <= 96);
    assert!(throttle.sleep_millis >= 12);
    assert!(reason.contains("external-or-secondary-volume"));
    assert!(reason.contains("cloud-root"));
    assert!(reason.contains("battery"));
    assert!(reason.contains("thermal-pressure"));
    assert!(reason.contains("network"));
}

fn must_ok<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{context}: {error}"),
    }
}

fn parse_json_value(json: &str, context: &str) -> serde_json::Value {
    match serde_json::from_str(json) {
        Ok(value) => value,
        Err(error) => panic!("{context}: {error}"),
    }
}

fn json_string<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    match value[key].as_str() {
        Some(value) => value,
        None => panic!("missing JSON string field: {key}"),
    }
}

fn test_root(name: &str) -> PathBuf {
    let millis = crate::current_unix_millis().unwrap_or_default();
    std::env::temp_dir().join(format!(
        "aetower-storage-{name}-{}-{millis}",
        std::process::id()
    ))
}

fn create_git_repo(repo: &Path, branch: &str) {
    let ref_path = repo.join(".git").join("refs").join("heads").join(branch);
    if let Some(parent) = ref_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create git refs: {error}");
    }
    if let Err(error) = fs::write(
        repo.join(".git").join("HEAD"),
        format!("ref: refs/heads/{branch}\n"),
    ) {
        panic!("write git head: {error}");
    }
    if let Err(error) = fs::write(ref_path, "1234567890abcdef1234567890abcdef12345678\n") {
        panic!("write git ref: {error}");
    }
}

fn test_growth_delta(
    path: &Path,
    repo_root: Option<&Path>,
    scan_millis: u64,
    previous_bytes: u64,
    current_bytes: u64,
) -> StorageGrowthDelta {
    StorageGrowthDelta {
        bucket_millis: (scan_millis / STORAGE_GROWTH_BUCKET_MILLIS) * STORAGE_GROWTH_BUCKET_MILLIS,
        scan_millis,
        path: path.display().to_string(),
        source_root: repo_root
            .unwrap_or_else(|| path.parent().unwrap_or(path))
            .display()
            .to_string(),
        repo_root: repo_root.map(|repo| repo.display().to_string()),
        repo_name: None,
        git_branch: None,
        git_head: None,
        kind: "rust-build".to_owned(),
        cleanup_tier: "rebuildable".to_owned(),
        previous_physical_bytes: previous_bytes,
        current_physical_bytes: current_bytes,
        delta_bytes: current_bytes as i64 - previous_bytes as i64,
        command: None,
        process_tree: None,
        ai_agent_session: None,
        writer_source: None,
        matched_writer_count: 0,
        attribution_sources: Vec::new(),
        attribution_confidence: "low".to_owned(),
        attribution_confidence_score: 0,
        attribution_ambiguous: false,
        attribution_summary: String::new(),
        attribution_evidence: Vec::new(),
    }
}

fn create_indexed_git_repo(repo: &Path) {
    if let Err(error) = fs::create_dir_all(repo) {
        panic!("create indexed git repo dir: {error}");
    }
    run_git(repo, &["init"]);
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("checkout")
        .arg("-b")
        .arg("main")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn write_complete_agent_contracts(repo: &Path) {
    write_minimal_agents_contract(repo);
    let agents_dir = repo.join(".agents");
    if let Err(error) = fs::create_dir_all(&agents_dir) {
        panic!("create .agents dir: {error}");
    }
    for (file, body) in [
        (
            "manifest",
            "contracts:\n  - id: agents_md\nintegrity:\n  unique_ids: true\n",
        ),
        ("repo-map", "roots: []\n"),
        ("commands", "commands: []\n"),
        ("references", "references: []\n"),
    ] {
        if let Err(error) = fs::write(
            agents_dir.join(format!("{file}.yaml")),
            format!("schema_version: 1\ngenerated_by: test\nsource_files:\n  - AGENTS.md\n{body}"),
        ) {
            panic!("write generated agent contract {file}: {error}");
        }
    }
    for (file, body) in [
        ("tasks", "tasks: []\n"),
        ("contracts", "contracts: []\n"),
        ("validation", "rules: []\n"),
        ("boundaries", "layers: []\nrules: []\n"),
        ("risks", "surfaces: []\n"),
    ] {
        if let Err(error) = fs::write(
            agents_dir.join(format!("{file}.yaml")),
            format!(
                "schema_version: 1\nreviewed_by: test\nreviewed_at: 2026-06-28\nsource_files:\n  - AGENTS.md\n{body}"
            ),
        ) {
            panic!("write reviewed agent contract {file}: {error}");
        }
    }
}

fn write_minimal_agents_contract(repo: &Path) {
    if let Err(error) = fs::write(repo.join("README.md"), "See AGENTS.md.\n") {
        panic!("write README.md: {error}");
    }
    if let Err(error) = fs::write(
        repo.join("AGENTS.md"),
        [
            "## Scope And Precedence",
            "Repository-local instructions override generic defaults.",
            "## Repository Map",
            "Use .agents/manifest.yaml for contract integrity, .agents/tasks.yaml for task routing, .agents/repo-map.yaml for topology, and .agents/contracts.yaml for invariants.",
            "## Standard Workflow",
            "Run git status --short before editing and preserve unrelated changes.",
            "## Commands",
            "Use .agents/commands.yaml for exact command metadata.",
            "## Approval Required",
            "Ask before destructive, network, deploy, migration, or setup actions.",
            "## Validation Matrix",
            "Use .agents/validation.yaml for touched-path validation mapping.",
            "## Architecture Boundaries",
            "Use .agents/boundaries.yaml for import and layer rules.",
            "## Code Rules",
            "Keep changes targeted and readable.",
            "## Security Rules",
            "Never commit secrets, credentials, production data, or local env files.",
            "## Completion Checklist",
            "Report changed files, validation, and residual risk.",
            "## References",
            "README.md",
        ]
        .join("\n\n"),
    ) {
        panic!("write AGENTS.md: {error}");
    }
}

fn git_add_all(repo: &Path) {
    run_git(repo, &["add", "README.md", "AGENTS.md"]);
    if repo.join(".agents").exists() {
        run_git(repo, &["add", ".agents"]);
    }
}

fn run_git(repo: &Path, args: &[&str]) {
    let status = match Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => status,
        Err(error) => panic!("run git {:?}: {error}", args),
    };
    if !status.success() {
        panic!("git {:?} failed with status {status}", args);
    }
}

fn mark_tree_old(path: &Path) {
    if !path.exists() {
        return;
    }
    if path.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) => panic!("read fixture tree: {error}"),
        };
        for entry in entries.flatten() {
            mark_tree_old(&entry.path());
        }
    }
    match Command::new("touch")
        .arg("-t")
        .arg("202001010000")
        .arg(path)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("set old fixture mtime failed: {status}"),
        Err(error) => panic!("run touch for fixture mtime: {error}"),
    }
}

fn write_git_origin_config(repo: &Path, origin_url: &str) {
    if let Err(error) = fs::write(
        repo.join(".git").join("config"),
        format!("[remote \"origin\"]\n\turl = {origin_url}\n"),
    ) {
        panic!("write git config: {error}");
    }
}

fn guidance_issue_ids(repo: &serde_json::Value) -> Vec<String> {
    repo["agent_guidance_issues"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|issue| issue["id"].as_str().map(str::to_owned))
        .collect()
}

fn test_storage_item(
    path: &str,
    kind: &str,
    storage_role: &str,
    safety: &str,
    cleanup_tier: &str,
    modified_millis: u64,
) -> StorageHygieneItem {
    StorageHygieneItem {
        id: path.to_owned(),
        path: path.to_owned(),
        display_name: Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact")
            .to_owned(),
        kind: kind.to_owned(),
        storage_role: storage_role.to_owned(),
        git_status: "outside-git".to_owned(),
        safety: safety.to_owned(),
        cleanup_tier: cleanup_tier.to_owned(),
        size_bytes: MIN_ITEM_BYTES + 128,
        logical_bytes: MIN_ITEM_BYTES + 128,
        physical_bytes: MIN_ITEM_BYTES + 128,
        byte_accounting: "test fixture".to_owned(),
        sparse_or_shared: false,
        hardlink_count: 1,
        has_hardlinks: false,
        cloud_placeholder: false,
        protected_path: is_protected_cleanup_path(path),
        size_truncated: false,
        modified_millis: Some(modified_millis),
        age_days: None,
        accessed_millis: None,
        access_age_days: None,
        cold: false,
        stale: true,
        reason: "test item".to_owned(),
        recommendation: "test recommendation".to_owned(),
        next_step: String::new(),
        command_hint: format!("du -sh {}", shell_quote(path)),
        rebuild_command: None,
        estimated_rebuild_cost: "Unknown".to_owned(),
        estimated_rebuild_seconds: None,
        cleanup_consequence: "test cleanup consequence".to_owned(),
        evidence: Vec::new(),
        cleanup_allowed: true,
        cleanup_blockers: Vec::new(),
        default_cleanup_action: "trash".to_owned(),
        attribution: StorageArtifactAttribution {
            repo_root: None,
            repo_name: None,
            git_branch: None,
            git_head: None,
            command: None,
            process_tree: None,
            ai_agent_session: None,
            confidence: "low".to_owned(),
            notes: Vec::new(),
        },
    }
}

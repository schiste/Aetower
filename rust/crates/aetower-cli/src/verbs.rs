//! Human-readable formatters for the curated verbs.
//!
//! Each function takes the tool's JSON payload (as returned by
//! [`crate::client::Client::fetch`]) and renders a compact operator view. Field
//! names here are matched to the live MCP payloads (`aetower_host_summary`,
//! `aetower_storage_hygiene_overview`, `aetower_repository_inventory`, …); the
//! `--json` path bypasses all of this and prints the raw structured payload.

use serde_json::Value;

use crate::table::{self, Column, human_bytes};

fn get_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn get_f64(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn get_u64(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn get_bool(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn get_arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `aetower top` — loudest entities right now (from `aetower_host_summary`).
pub fn top(payload: &Value, limit: usize) -> String {
    let cols = [
        Column::left("ENTITY"),
        Column::right("FRICTION"),
        Column::right("CPU%"),
        Column::right("MEM"),
        Column::left("BADGES"),
    ];
    let rows: Vec<Vec<String>> = get_arr(payload, "top_entities")
        .iter()
        .take(limit)
        .map(|entity| {
            let badges = get_arr(entity, "badges")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            vec![
                truncate(&get_str(entity, "display_name"), 40),
                format!("{:.0}", get_f64(entity, "friction")),
                format!("{:.0}", get_f64(entity, "cpu_percent")),
                human_bytes(get_u64(entity, "memory_bytes")),
                truncate(&badges, 32),
            ]
        })
        .collect();
    table::render(
        &cols,
        &rows,
        "No entities reported (is the app warming up?).",
    )
}

/// `aetower findings` — the recommendation feed (`aetower_top_findings`).
pub fn findings(payload: &Value) -> String {
    let cols = [
        Column::left("SEVERITY"),
        Column::left("TITLE"),
        Column::left("RECOMMENDATION"),
    ];
    let rows: Vec<Vec<String>> = get_arr(payload, "findings")
        .iter()
        .map(|f| {
            vec![
                get_str(f, "severity"),
                truncate(&get_str(f, "title"), 44),
                truncate(&get_str(f, "recommendation"), 60),
            ]
        })
        .collect();
    table::render(
        &cols,
        &rows,
        "No findings — nothing is straining the machine.",
    )
}

/// `aetower alerts` — active alerts and budget breaches (`aetower_host_alerts`).
pub fn alerts(payload: &Value) -> String {
    let cols = [
        Column::left("SEVERITY"),
        Column::left("CATEGORY"),
        Column::left("TITLE"),
    ];
    let rows: Vec<Vec<String>> = get_arr(payload, "alerts")
        .iter()
        .map(|a| {
            vec![
                get_str(a, "severity"),
                get_str(a, "category"),
                truncate(&get_str(a, "title"), 60),
            ]
        })
        .collect();
    table::render(&cols, &rows, "No active alerts.")
}

/// True when any alert is above informational severity — the `--fail-on-warn`
/// gate.
pub fn alerts_breached(payload: &Value) -> bool {
    get_arr(payload, "alerts").iter().any(|a| {
        let severity = get_str(a, "severity").to_lowercase();
        matches!(
            severity.as_str(),
            "warning" | "warn" | "critical" | "error" | "serious"
        )
    })
}

/// `aetower host` — the host scalar summary as aligned key/value lines.
pub fn host(payload: &Value) -> String {
    let host = payload.get("host").cloned().unwrap_or(Value::Null);
    let mem_used = get_u64(&host, "memory_used_bytes");
    let mem_total = get_u64(&host, "memory_total_bytes");
    let pairs: Vec<(&str, String)> = vec![
        ("CPU", format!("{:.0}%", get_f64(&host, "cpu_percent"))),
        (
            "Memory",
            format!("{} / {}", human_bytes(mem_used), human_bytes(mem_total)),
        ),
        ("Swap", human_bytes(get_u64(&host, "swap_used_bytes"))),
        ("GPU", format!("{:.0}%", get_f64(&host, "gpu_percent"))),
        (
            "Wakeups",
            format!("{:.0}/s", get_f64(&host, "wakeups_per_second")),
        ),
        (
            "Disk",
            format!(
                "{}/s read · {}/s write",
                human_bytes(get_u64(&host, "disk_read_bps")),
                human_bytes(get_u64(&host, "disk_write_bps"))
            ),
        ),
        (
            "Network",
            format!(
                "{}/s down · {}/s up",
                human_bytes(get_u64(&host, "network_receive_bps")),
                human_bytes(get_u64(&host, "network_send_bps"))
            ),
        ),
        ("Thermal", get_str(&host, "thermal_state")),
        (
            "Power",
            if host
                .get("on_battery")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                format!("battery {}%", get_u64(&host, "battery_charge_percent"))
            } else {
                "AC".to_string()
            },
        ),
        ("AI agents", get_u64(&host, "ai_agent_count").to_string()),
    ];
    let width = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    pairs
        .iter()
        .map(|(k, v)| format!("{k:<width$}  {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `aetower storage` — disk pressure + reclaim lanes (`aetower_storage_hygiene_overview`).
pub fn storage(payload: &Value) -> String {
    let mut out = String::new();

    for volume in get_arr(payload, "volume_states") {
        let path = get_str(volume, "path");
        let free = get_u64(volume, "free_now_bytes").max(get_u64(volume, "available_bytes"));
        let total = get_u64(volume, "total_bytes");
        out.push_str(&format!(
            "Volume {}: {} free of {}\n",
            if path.is_empty() { "/" } else { &path },
            human_bytes(free),
            human_bytes(total)
        ));
    }

    let summary = payload.get("summary").cloned().unwrap_or(Value::Null);
    let inventory = get_u64(&summary, "inventory_size_bytes");
    let safe_now = get_u64(&summary, "safely_reclaimable_now_bytes");
    let maybe = get_u64(&summary, "maybe_reclaimable_bytes");
    let review = get_u64(&summary, "review_required_bytes");
    let dangerous = get_u64(&summary, "dangerous_user_data_bytes");
    let items = get_u64(&summary, "item_count");
    let cache_status = payload.get("cache_status").cloned().unwrap_or(Value::Null);
    let cache_source = get_str(&cache_status, "source");
    let confidence = get_str(&cache_status, "confidence");
    if !cache_source.is_empty() {
        out.push_str(&format!(
            "Cache: {} · stale {} · partial {} · {} confidence\n",
            cache_source,
            if get_bool(&cache_status, "stale") {
                "yes"
            } else {
                "no"
            },
            if get_bool(&cache_status, "partial") {
                "yes"
            } else {
                "no"
            },
            if confidence.is_empty() {
                "unknown"
            } else {
                &confidence
            }
        ));
        if let Some(background_scan) = cache_status.get("background_scan") {
            let job_id = get_str(background_scan, "job_id");
            let status = get_str(background_scan, "status");
            if !job_id.is_empty() || !status.is_empty() {
                out.push_str(&format!(
                    "Refresh: {} {}\n",
                    if job_id.is_empty() {
                        "background-scan"
                    } else {
                        &job_id
                    },
                    if status.is_empty() {
                        "requested"
                    } else {
                        &status
                    }
                ));
            }
        }
    }
    out.push_str(&format!(
        "Inventory: {} across {} items · Safe now: {} · Maybe: {} · Review: {} · Dangerous/user data: {}",
        human_bytes(inventory),
        items,
        human_bytes(safe_now),
        human_bytes(maybe),
        human_bytes(review),
        human_bytes(dangerous)
    ));
    let largest_path = get_str(&summary, "largest_item_path");
    if !largest_path.is_empty() {
        out.push_str(&format!(
            "  (largest: {}, {})",
            truncate(&largest_path, 48),
            human_bytes(get_u64(&summary, "largest_item_bytes"))
        ));
    }
    out.push_str("\n\n");

    let cols = [
        Column::left("LANE"),
        Column::right("ITEMS"),
        Column::right("RECLAIMABLE"),
    ];
    let rows: Vec<Vec<String>> = get_arr(payload, "cleanup_tiers")
        .iter()
        .map(|t| {
            vec![
                get_str(t, "label"),
                get_u64(t, "item_count").to_string(),
                human_bytes(get_u64(t, "bytes")),
            ]
        })
        .collect();
    out.push_str(&table::render(&cols, &rows, "No reclaim lanes found."));
    out
}

/// `aetower repos` — repository health (`aetower_repository_inventory`).
pub fn repos(payload: &Value, limit: usize) -> String {
    let cols = [
        Column::left("REPOSITORY"),
        Column::left("BRANCH"),
        Column::left("DIRTY"),
        Column::left("READINESS"),
        Column::right("CLONES"),
    ];
    let rows: Vec<Vec<String>> = get_arr(payload, "repository_inventory")
        .iter()
        .take(limit)
        .map(|r| {
            let dirty = match get_str(r, "git_dirty_status").as_str() {
                "" | "not_checked_lazy" => "—".to_string(),
                "clean" => "clean".to_string(),
                other => {
                    let n = get_u64(r, "git_dirty_file_count");
                    if n > 0 {
                        format!("{other} ({n})")
                    } else {
                        other.to_string()
                    }
                }
            };
            let clones = get_u64(r, "clone_group_count");
            vec![
                truncate(&get_str(r, "repo_name"), 32),
                truncate(&get_str(r, "git_branch"), 20),
                dirty,
                get_str(r, "agent_readiness_status"),
                if clones > 1 {
                    clones.to_string()
                } else {
                    String::new()
                },
            ]
        })
        .collect();
    table::render(
        &cols,
        &rows,
        "No repositories discovered under the scanned roots.",
    )
}

/// `aetower doctor` — reachability, capabilities, and engine health.
pub fn doctor(host_summary: &Value, diagnostics: &Value, socket: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("Socket:   {socket}  (reachable)\n"));
    out.push_str(&format!(
        "Engine:   live · {} snapshots published\n\n",
        get_u64(host_summary, "sequence")
    ));

    if let Some(caps) = host_summary
        .get("capability_states")
        .and_then(Value::as_object)
    {
        let cols = [Column::left("CAPABILITY"), Column::left("STATE")];
        let mut rows: Vec<Vec<String>> = caps
            .iter()
            .map(|(k, v)| vec![k.clone(), v.as_str().unwrap_or("?").to_string()])
            .collect();
        rows.sort_by(|a, b| a[0].cmp(&b[0]));
        out.push_str(&table::render(
            &cols,
            &rows,
            "No capability states reported.",
        ));
        out.push_str("\n\n");
    }

    let errors = get_u64(diagnostics, "error_count");
    let warns = get_u64(diagnostics, "warn_count");
    let dropped = get_u64(diagnostics, "dropped_events");
    out.push_str(&format!(
        "Diagnostics: {errors} errors · {warns} warnings · {dropped} dropped events",
    ));
    let last_error = get_str(diagnostics, "last_error_message");
    if !last_error.is_empty() {
        out.push_str(&format!("\nLast error:  {}", truncate(&last_error, 80)));
    }
    out
}

/// True only when the engine itself is failing to persist diagnostics — a real
/// unhealthy signal, unlike the cumulative `error_count` of recorded incidents
/// (which are logged events, not a broken app).
pub fn doctor_unhealthy(diagnostics: &Value) -> bool {
    diagnostics
        .get("persistence_error")
        .map(|value| !value.is_null())
        .unwrap_or(false)
}

/// `aetower tools` — the tool catalog (`tools/list`).
pub fn tools(payload: &Value) -> String {
    let cols = [Column::left("TOOL"), Column::left("DESCRIPTION")];
    let rows: Vec<Vec<String>> = get_arr(payload, "tools")
        .iter()
        .map(|t| vec![get_str(t, "name"), truncate(&get_str(t, "description"), 76)])
        .collect();
    let mut out = table::render(&cols, &rows, "No tools advertised.");
    out.push_str(&format!(
        "\n\n{} tools available.",
        get_arr(payload, "tools").len()
    ));
    out
}

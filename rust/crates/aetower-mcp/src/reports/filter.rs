//! Sandboxed user-defined process filters, evaluated with Rhai.
//!
//! A filter is a single boolean expression evaluated once per process, with the
//! process's fields exposed as scope variables (`name`, `path`, `cmd`, `user`,
//! `cwd`, `entity`, `bundle`, `badges`, `pid`, `cpu`, `mem`, `mem_mb`,
//! `threads`, `energy`, `friction`). An entity matches when any of its
//! processes match. `compile_expression` forbids statements/loops/function
//! definitions, and `set_max_operations` bounds runaway evaluation.

use crate::*;
use rhai::{Engine, Scope};

#[derive(Debug, Clone, Serialize)]
struct EntityFilterReport {
    expression: String,
    matched_entity_ids: Vec<String>,
    matched_pids: Vec<u32>,
    matched_count: u32,
    evaluated_entities: u32,
    error: Option<String>,
}

/// Build a hardened Rhai engine. Numeric process fields are exposed as `f64`,
/// so we register mixed `f64`/`i64` comparison operators to make natural
/// integer-literal comparisons (`cpu > 50`) work without type errors.
fn build_engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(50_000);
    engine.set_max_expr_depths(32, 32);
    engine.set_max_string_size(16 * 1024);

    macro_rules! mixed_cmp {
        ($op:tt) => {{
            engine.register_fn(stringify!($op), |a: f64, b: i64| a $op (b as f64));
            engine.register_fn(stringify!($op), |a: i64, b: f64| (a as f64) $op b);
        }};
    }
    mixed_cmp!(>);
    mixed_cmp!(>=);
    mixed_cmp!(<);
    mixed_cmp!(<=);
    mixed_cmp!(==);
    mixed_cmp!(!=);
    engine
}

pub fn filter_entities_json(
    data_source: &dyn AetowerMcpDataSource,
    expression: &str,
) -> Result<String, String> {
    let snapshot = data_source.latest_snapshot()?;
    let report = build_entity_filter(&snapshot, expression)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn build_entity_filter(
    snapshot: &SystemSnapshot,
    expression: &str,
) -> Result<EntityFilterReport, String> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return Err("Filter expression is empty.".to_owned());
    }

    let engine = build_engine();
    // A single-expression compile rejects statements, loops, and `fn` defs.
    let ast = engine
        .compile_expression(trimmed)
        .map_err(|error| format!("Invalid filter expression: {error}"))?;

    let mut matched_entity_ids = Vec::new();
    let mut matched_pids = Vec::new();
    let mut eval_error: Option<String> = None;
    let mut evaluated_entities = 0u32;

    'entities: for entity in &snapshot.entities {
        evaluated_entities += 1;
        let mut entity_matched = false;
        for component in &entity.components {
            if component.kind == aetower_model::ComponentKind::AdapterContext {
                continue;
            }
            let mut scope = Scope::new();
            scope.push("name", component.title.clone());
            scope.push(
                "path",
                component.executable_path.clone().unwrap_or_default(),
            );
            scope.push("cmd", component.command_line.clone().unwrap_or_default());
            scope.push("user", component.user.clone().unwrap_or_default());
            scope.push("cwd", component.cwd.clone().unwrap_or_default());
            scope.push("entity", entity.display_name.clone());
            scope.push("bundle", entity.bundle_id.clone().unwrap_or_default());
            scope.push("badges", entity.badges.join(" "));
            scope.push("pid", component.process_id.unwrap_or(0) as f64);
            scope.push("cpu", component.cpu_percent as f64);
            scope.push("mem", component.memory_bytes as f64);
            scope.push("mem_mb", component.memory_bytes as f64 / (1024.0 * 1024.0));
            scope.push("threads", component.thread_count as f64);
            scope.push("energy", entity.metrics.energy_nj_per_s);
            scope.push("friction", entity.friction.total_score as f64);

            match engine.eval_ast_with_scope::<bool>(&mut scope, &ast) {
                Ok(true) => {
                    entity_matched = true;
                    if let Some(pid) = component.process_id {
                        matched_pids.push(pid);
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    eval_error = Some(format!("Filter evaluation failed: {error}"));
                    break 'entities;
                }
            }
        }
        if entity_matched {
            matched_entity_ids.push(entity.entity_id.clone());
        }
    }

    matched_pids.sort_unstable();
    matched_pids.dedup();

    Ok(EntityFilterReport {
        expression: trimmed.to_owned(),
        matched_count: matched_entity_ids.len() as u32,
        matched_entity_ids,
        matched_pids,
        evaluated_entities,
        error: eval_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aetower_model::{
        AggregateMetrics, ComponentKind, ComponentSnapshot, EntityKind, EntitySnapshot,
        FrictionBreakdown, MetricTrend, SystemSnapshot,
    };

    fn component(title: &str, pid: u32, cpu: f32, threads: u32) -> ComponentSnapshot {
        ComponentSnapshot {
            kind: ComponentKind::Process,
            title: title.to_owned(),
            detail: String::new(),
            adapter_context: None,
            provenance: None,
            process_id: Some(pid),
            start_time_millis: 0,
            executable_path: Some(format!("/Applications/{title}.app/Contents/MacOS/{title}")),
            command_line: None,
            parent_summary: None,
            launched_by: None,
            cpu_percent: cpu,
            memory_bytes: 256 * 1024 * 1024,
            memory_physical_footprint_bytes: 0,
            cwd: None,
            user: Some("alice".to_owned()),
            thread_count: threads,
        }
    }

    fn snapshot() -> SystemSnapshot {
        let entity = EntitySnapshot {
            entity_id: "e1".to_owned(),
            display_name: "Test".to_owned(),
            entity_kind: EntityKind::App,
            metrics: AggregateMetrics::default(),
            friction: FrictionBreakdown::default(),
            trend: MetricTrend::default(),
            components: vec![
                component("Helper", 1001, 80.0, 12),
                component("Main", 1002, 5.0, 3),
            ],
            ..Default::default()
        };
        SystemSnapshot {
            entities: vec![entity],
            ..Default::default()
        }
    }

    #[test]
    fn matches_on_numeric_comparison_with_integer_literal() {
        let Ok(report) = build_entity_filter(&snapshot(), "cpu > 50") else {
            panic!("filter should evaluate");
        };
        assert_eq!(report.matched_entity_ids, vec!["e1".to_owned()]);
        assert_eq!(report.matched_pids, vec![1001]);
        assert!(report.error.is_none());
    }

    #[test]
    fn matches_on_string_and_threads() {
        let Ok(report) =
            build_entity_filter(&snapshot(), "name.contains(\"Help\") && threads > 10")
        else {
            panic!("filter should evaluate");
        };
        assert_eq!(report.matched_pids, vec![1001]);
    }

    #[test]
    fn no_match_returns_empty() {
        let Ok(report) = build_entity_filter(&snapshot(), "cpu > 99") else {
            panic!("filter should evaluate");
        };
        assert!(report.matched_entity_ids.is_empty());
        assert_eq!(report.matched_count, 0);
    }

    #[test]
    fn invalid_expression_is_rejected() {
        assert!(build_entity_filter(&snapshot(), "cpu >").is_err());
    }

    #[test]
    fn statements_are_rejected() {
        // compile_expression forbids statements / loops / fn defs.
        assert!(build_entity_filter(&snapshot(), "let x = 1; x > 0").is_err());
    }
}

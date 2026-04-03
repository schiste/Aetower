use std::{fs, path::PathBuf};

use aetower_core::{RawProcessSample, run_entity_pipeline};
use aetower_model::{EntityKind, FrontmostAppState, HostSnapshot, ProvenanceKind};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct PipelineFixture {
    host: HostSnapshot,
    #[serde(default)]
    frontmost: Option<FrontmostAppState>,
    processes: Vec<RawProcessSample>,
    expected_entities: Vec<ExpectedEntity>,
}

#[derive(Debug, Deserialize)]
struct ExpectedEntity {
    display_name: String,
    entity_kind: EntityKind,
    primary_provenance: Option<ProvenanceKind>,
    #[serde(default)]
    component_expectations: Vec<ExpectedComponent>,
}

#[derive(Debug, Deserialize)]
struct ExpectedComponent {
    title: String,
    provenance: Option<ProvenanceKind>,
    launched_by_contains: Option<String>,
}

#[test]
fn browser_helper_fixture_matches_expected_pipeline_output() {
    assert_fixture("browser_helper.json");
}

#[test]
fn terminal_and_launchd_fixture_matches_expected_pipeline_output() {
    assert_fixture("terminal_and_launchd.json");
}

#[test]
fn electron_user_launch_fixture_matches_expected_pipeline_output() {
    assert_fixture("electron_user_launch.json");
}

#[test]
fn xpc_service_fixture_matches_expected_pipeline_output() {
    assert_fixture("xpc_service.json");
}

#[test]
fn safari_webkit_fixture_matches_expected_pipeline_output() {
    assert_fixture("safari_webkit.json");
}

#[test]
fn login_item_fixture_matches_expected_pipeline_output() {
    assert_fixture("login_item.json");
}

fn assert_fixture(file_name: &str) {
    let fixture = load_fixture(file_name);
    let output = run_entity_pipeline(
        &fixture.processes,
        &fixture.host,
        fixture.frontmost.as_ref(),
    );

    for expected in &fixture.expected_entities {
        let entity = output
            .entities
            .iter()
            .find(|entity| entity.display_name == expected.display_name)
            .unwrap_or_else(|| panic!("missing expected entity {}", expected.display_name));

        assert_eq!(entity.entity_kind, expected.entity_kind);
        assert_eq!(
            entity.primary_provenance.as_ref().map(|value| &value.kind),
            expected.primary_provenance.as_ref()
        );

        for component_expectation in &expected.component_expectations {
            let component = entity
                .components
                .iter()
                .find(|component| component.title == component_expectation.title)
                .unwrap_or_else(|| {
                    panic!(
                        "missing expected component {} on {}",
                        component_expectation.title, expected.display_name
                    )
                });

            assert_eq!(
                component.provenance.as_ref().map(|value| &value.kind),
                component_expectation.provenance.as_ref()
            );
            if let Some(expected_launcher_fragment) =
                component_expectation.launched_by_contains.as_deref()
            {
                let launched_by = component
                    .launched_by
                    .as_deref()
                    .expect("expected launched_by to be present");
                assert!(
                    launched_by.contains(expected_launcher_fragment),
                    "expected '{launched_by}' to contain '{expected_launcher_fragment}'"
                );
            }
        }
    }
}

fn load_fixture(file_name: &str) -> PipelineFixture {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(file_name);
    let raw = fs::read_to_string(&fixture_path).unwrap_or_else(|error| {
        panic!("failed to read fixture {}: {error}", fixture_path.display())
    });
    serde_json::from_str(&raw).unwrap_or_else(|error| {
        panic!(
            "failed to parse fixture {}: {error}",
            fixture_path.display()
        )
    })
}

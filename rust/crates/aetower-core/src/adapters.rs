use std::{collections::BTreeMap, env, path::Path};

use aetower_model::{CapabilityKind, CapabilitySnapshot, CapabilityState, ComponentKind, ComponentSnapshot, EntitySnapshot};

#[derive(Default)]
pub struct AdapterManager;

impl AdapterManager {
    pub fn initial_capabilities() -> BTreeMap<CapabilityKind, CapabilitySnapshot> {
        let now = crate::time::now_millis();
        let docker_available = Path::new("/var/run/docker.sock").exists();
        let chromium_available = env::var("AETOWER_CHROMIUM_ENDPOINT").is_ok();

        BTreeMap::from([
            (
                CapabilityKind::Accessibility,
                CapabilitySnapshot {
                    kind: CapabilityKind::Accessibility,
                    state: CapabilityState::Unknown,
                    detail: "Required for richer UI-state correlation and future window context.".to_owned(),
                    last_updated_millis: now,
                },
            ),
            (
                CapabilityKind::FullDiskAccess,
                CapabilitySnapshot {
                    kind: CapabilityKind::FullDiskAccess,
                    state: CapabilityState::Unknown,
                    detail: "Optional. Improves origin and metadata access for protected locations.".to_owned(),
                    last_updated_millis: now,
                },
            ),
            (
                CapabilityKind::AppleAutomation,
                CapabilitySnapshot {
                    kind: CapabilityKind::AppleAutomation,
                    state: CapabilityState::Unknown,
                    detail: "Optional. Enables scriptable-app enrichments like media context.".to_owned(),
                    last_updated_millis: now,
                },
            ),
            (
                CapabilityKind::ChromiumDebug,
                CapabilitySnapshot {
                    kind: CapabilityKind::ChromiumDebug,
                    state: if chromium_available {
                        CapabilityState::Granted
                    } else {
                        CapabilityState::Unavailable
                    },
                    detail: if chromium_available {
                        "Chromium debugging endpoint configured via AETOWER_CHROMIUM_ENDPOINT.".to_owned()
                    } else {
                        "Set AETOWER_CHROMIUM_ENDPOINT to enable browser-target enrichment.".to_owned()
                    },
                    last_updated_millis: now,
                },
            ),
            (
                CapabilityKind::DockerSocket,
                CapabilitySnapshot {
                    kind: CapabilityKind::DockerSocket,
                    state: if docker_available {
                        CapabilityState::Granted
                    } else {
                        CapabilityState::Unavailable
                    },
                    detail: if docker_available {
                        "Docker socket detected.".to_owned()
                    } else {
                        "Docker socket not detected at /var/run/docker.sock.".to_owned()
                    },
                    last_updated_millis: now,
                },
            ),
        ])
    }

    pub fn enrich_entities(
        &self,
        entities: &mut [EntitySnapshot],
        capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>,
    ) {
        for entity in entities {
            if matches!(entity.entity_kind, aetower_model::EntityKind::TerminalSession) {
                entity.badges.push("command-attributed".to_owned());
            }

            if entity.display_name.contains("Docker") {
                if let Some(capability) = capabilities.get(&CapabilityKind::DockerSocket) {
                    entity.components.push(ComponentSnapshot {
                        kind: ComponentKind::AdapterContext,
                        title: "Docker context".to_owned(),
                        detail: capability.detail.clone(),
                        cpu_percent: 0.0,
                        memory_bytes: 0,
                    });
                }
            }

            if matches!(entity.entity_kind, aetower_model::EntityKind::Browser) {
                if let Some(capability) = capabilities.get(&CapabilityKind::ChromiumDebug) {
                    entity.components.push(ComponentSnapshot {
                        kind: ComponentKind::AdapterContext,
                        title: "Browser enrichment".to_owned(),
                        detail: capability.detail.clone(),
                        cpu_percent: 0.0,
                        memory_bytes: 0,
                    });
                }
            }
        }
    }
}

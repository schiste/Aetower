use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    io::{Read, Write},
    net::TcpStream,
    os::unix::net::UnixStream,
    path::Path,
    process::Command,
    sync::Arc,
    time::Duration,
};

use aetower_model::{
    CapabilityKind, CapabilitySnapshot, CapabilityState, ComponentKind, ComponentSnapshot,
    EntitySnapshot,
};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use tungstenite::{connect, Message};
use url::Url;

const CHROMIUM_TIMEOUT: Duration = Duration::from_millis(300);
const DOCKER_TIMEOUT: Duration = Duration::from_millis(300);

#[derive(Clone)]
pub struct AdapterManager {
    state: Arc<Mutex<AdapterState>>,
}

#[derive(Debug, Default)]
struct AdapterState {
    chromium_endpoint: Option<String>,
    docker_socket_path: String,
    privileged_helper_path: Option<String>,
    privileged_helper_enabled: bool,
    chromium_samples: BTreeMap<String, ChromiumRuntimeSample>,
}

#[derive(Debug, Clone)]
struct ChromiumAdapterConfig {
    host: String,
    port: u16,
    path: String,
}

#[derive(Debug, Clone)]
struct DockerAdapterConfig {
    socket_path: String,
}

#[derive(Debug, Clone)]
struct ChromiumRuntimeSample {
    captured_at_millis: u64,
    task_duration_seconds: f64,
    network_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct ChromiumTarget {
    #[serde(rename = "type")]
    target_type: Option<String>,
    id: Option<String>,
    title: Option<String>,
    url: Option<String>,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DockerContainerSummary {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Names")]
    names: Vec<String>,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Ports", default)]
    ports: Vec<DockerPortSummary>,
}

#[derive(Debug, Deserialize)]
struct DockerPortSummary {
    #[serde(rename = "IP")]
    ip: Option<String>,
    #[serde(rename = "PrivatePort")]
    private_port: u16,
    #[serde(rename = "PublicPort")]
    public_port: Option<u16>,
    #[serde(rename = "Type")]
    port_type: String,
}

#[derive(Debug, Deserialize)]
struct DockerContainerStats {
    #[serde(rename = "cpu_stats")]
    cpu_stats: DockerCpuStats,
    #[serde(rename = "precpu_stats")]
    pre_cpu_stats: DockerCpuStats,
    #[serde(rename = "memory_stats")]
    memory_stats: DockerMemoryStats,
    #[serde(default)]
    networks: BTreeMap<String, DockerNetworkStats>,
    #[serde(rename = "blkio_stats")]
    blkio_stats: DockerBlkioStats,
    #[serde(rename = "pids_stats")]
    pids_stats: DockerPidsStats,
}

#[derive(Debug, Default, Deserialize)]
struct DockerCpuStats {
    #[serde(rename = "cpu_usage")]
    cpu_usage: DockerCpuUsage,
    system_cpu_usage: Option<u64>,
    online_cpus: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerCpuUsage {
    total_usage: u64,
    #[serde(default)]
    percpu_usage: Vec<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerMemoryStats {
    usage: Option<u64>,
    limit: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerNetworkStats {
    rx_bytes: u64,
    tx_bytes: u64,
}

#[derive(Debug, Default, Deserialize)]
struct DockerBlkioStats {
    #[serde(default, rename = "io_service_bytes_recursive")]
    io_service_bytes_recursive: Vec<DockerBlkioEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerBlkioEntry {
    op: Option<String>,
    value: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerPidsStats {
    current: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct PrivilegedHelperSnapshot {
    processes: Vec<PrivilegedProcessSample>,
}

#[derive(Debug, Deserialize)]
struct PrivilegedProcessSample {
    pid: u32,
    process_name: String,
    executable_name: Option<String>,
    connections: Vec<String>,
}

impl Default for AdapterManager {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(AdapterState {
                chromium_endpoint: env::var("AETOWER_CHROMIUM_ENDPOINT").ok(),
                docker_socket_path: docker_socket_path(),
                privileged_helper_path: env::var("AETOWER_PRIVILEGED_HELPER").ok(),
                privileged_helper_enabled: env::var("AETOWER_PRIVILEGED_HELPER_ENABLED")
                    .ok()
                    .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                    .unwrap_or(false),
                chromium_samples: BTreeMap::new(),
            })),
        }
    }
}

impl AdapterManager {
    pub fn initial_capabilities(&self) -> BTreeMap<CapabilityKind, CapabilitySnapshot> {
        let now = crate::time::now_millis();
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
                self.capability_snapshot(CapabilityKind::ChromiumDebug, now),
            ),
            (
                CapabilityKind::DockerSocket,
                self.capability_snapshot(CapabilityKind::DockerSocket, now),
            ),
            (
                CapabilityKind::PrivilegedHelper,
                self.capability_snapshot(CapabilityKind::PrivilegedHelper, now),
            ),
        ])
    }

    pub fn capability_snapshot(
        &self,
        kind: CapabilityKind,
        last_updated_millis: u64,
    ) -> CapabilitySnapshot {
        let guard = self.state.lock();
        let (state, detail) = capability_status(&guard, &kind);
        CapabilitySnapshot {
            kind,
            state,
            detail,
            last_updated_millis,
        }
    }

    pub fn configure_chromium_endpoint(&self, endpoint: Option<String>) {
        let mut guard = self.state.lock();
        guard.chromium_endpoint = endpoint.filter(|value| !value.trim().is_empty());
        guard.chromium_samples.clear();
    }

    pub fn configure_docker_socket_path(&self, socket_path: String) {
        let mut guard = self.state.lock();
        guard.docker_socket_path = if socket_path.trim().is_empty() {
            "/var/run/docker.sock".to_owned()
        } else {
            socket_path
        };
    }

    pub fn configure_privileged_helper(&self, helper_path: Option<String>, enabled: bool) {
        let mut guard = self.state.lock();
        guard.privileged_helper_path = helper_path.filter(|value| !value.trim().is_empty());
        guard.privileged_helper_enabled = enabled;
    }

    pub fn enrich_entities(
        &self,
        entities: &mut [EntitySnapshot],
        capabilities: &BTreeMap<CapabilityKind, CapabilitySnapshot>,
    ) {
        let mut guard = self.state.lock();

        let chromium_targets = capabilities
            .get(&CapabilityKind::ChromiumDebug)
            .filter(|capability| capability.state == CapabilityState::Granted)
            .and_then(|_| guard.chromium_config())
            .and_then(|config| fetch_chromium_targets(&config, &mut guard.chromium_samples).ok());

        let docker_containers = capabilities
            .get(&CapabilityKind::DockerSocket)
            .filter(|capability| capability.state == CapabilityState::Granted)
            .map(|_| DockerAdapterConfig {
                socket_path: guard.docker_socket_path.clone(),
            })
            .and_then(|config| fetch_docker_containers(&config).ok());

        let privileged_helper_sample = capabilities
            .get(&CapabilityKind::PrivilegedHelper)
            .filter(|capability| capability.state == CapabilityState::Granted)
            .and_then(|_| guard.privileged_helper_path())
            .and_then(|path| fetch_privileged_helper_sample(&path).ok());

        drop(guard);

        for entity in entities {
            if matches!(entity.entity_kind, aetower_model::EntityKind::TerminalSession) {
                if !entity.badges.iter().any(|badge| badge == "command-attributed") {
                    entity.badges.push("command-attributed".to_owned());
                }
            }

            if matches!(entity.entity_kind, aetower_model::EntityKind::Browser) {
                if let Some(targets) = chromium_targets.as_ref() {
                    for target in targets.iter().take(5) {
                        entity.components.push(ComponentSnapshot {
                            kind: ComponentKind::AdapterContext,
                            title: format!("Tab {} · {}", target.id, target.title),
                            detail: chromium_target_detail(target),
                            cpu_percent: target.cpu_percent,
                            memory_bytes: target.js_heap_used_bytes,
                        });
                    }
                    if !targets.is_empty() && !entity.badges.iter().any(|badge| badge == "chromium-live") {
                        entity.badges.push("chromium-live".to_owned());
                    }
                }
            }

            if entity.display_name.contains("Docker") || entity.display_name == "com.docker.backend" {
                if let Some(containers) = docker_containers.as_ref() {
                    for container in containers.iter().take(5) {
                        entity.components.push(ComponentSnapshot {
                            kind: ComponentKind::AdapterContext,
                            title: container.name.clone(),
                            detail: docker_container_detail(container),
                            cpu_percent: container.cpu_percent,
                            memory_bytes: container.memory_usage_bytes,
                        });
                    }
                    if !containers.is_empty() && !entity.badges.iter().any(|badge| badge == "docker-live") {
                        entity.badges.push("docker-live".to_owned());
                    }
                }
            }

            if let Some(helper) = privileged_helper_sample.as_ref() {
                if let Some(process) = helper
                    .processes
                    .iter()
                    .find(|process| helper_process_matches(entity, process))
                {
                    entity.components.push(ComponentSnapshot {
                        kind: ComponentKind::AdapterContext,
                        title: format!("Privileged sockets · {}", process.process_name),
                        detail: format!(
                            "pid {} · {}",
                            process.pid,
                            process.connections.join(" · ")
                        ),
                        cpu_percent: 0.0,
                        memory_bytes: 0,
                    });
                    if !entity.badges.iter().any(|badge| badge == "privileged-helper") {
                        entity.badges.push("privileged-helper".to_owned());
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ChromiumPageTarget {
    id: String,
    title: String,
    url: String,
    debug_socket: Option<String>,
    js_heap_used_bytes: u64,
    js_heap_total_bytes: u64,
    dom_nodes: u64,
    documents: u64,
    frames: u64,
    cpu_percent: f32,
    network_bps: u64,
}

#[derive(Debug, Clone)]
struct DockerContainer {
    id: String,
    name: String,
    image: String,
    status: String,
    ports: Vec<String>,
    cpu_percent: f32,
    memory_usage_bytes: u64,
    memory_limit_bytes: u64,
    network_rx_bytes: u64,
    network_tx_bytes: u64,
    block_read_bytes: u64,
    block_write_bytes: u64,
    pids: u64,
}

#[derive(Debug, Default)]
struct ChromiumMetrics {
    js_heap_used_bytes: u64,
    js_heap_total_bytes: u64,
    dom_nodes: u64,
    documents: u64,
    frames: u64,
    task_duration_seconds: f64,
    network_bytes: u64,
}

impl AdapterState {
    fn chromium_config(&self) -> Option<ChromiumAdapterConfig> {
        self.chromium_endpoint
            .as_deref()
            .and_then(parse_http_endpoint)
            .map(|(host, port, path)| ChromiumAdapterConfig { host, port, path })
    }

    fn privileged_helper_path(&self) -> Option<String> {
        if self.privileged_helper_enabled {
            self.privileged_helper_path.clone()
        } else {
            None
        }
    }
}

fn capability_status(state: &AdapterState, kind: &CapabilityKind) -> (CapabilityState, String) {
    match kind {
        CapabilityKind::ChromiumDebug => match state.chromium_endpoint.as_deref() {
            Some(raw) if parse_http_endpoint(raw).is_some() => (
                CapabilityState::Granted,
                format!("Chromium target discovery configured at {raw}."),
            ),
            Some(raw) => (
                CapabilityState::Denied,
                format!("Chromium endpoint is invalid: {raw}."),
            ),
            None => (
                CapabilityState::Unavailable,
                "Set a local Chromium /json or /json/list endpoint in settings.".to_owned(),
            ),
        },
        CapabilityKind::DockerSocket => {
            if Path::new(&state.docker_socket_path).exists() {
                (
                    CapabilityState::Granted,
                    format!("Docker socket detected at {}.", state.docker_socket_path),
                )
            } else {
                (
                    CapabilityState::Unavailable,
                    format!("Docker socket not detected at {}.", state.docker_socket_path),
                )
            }
        }
        CapabilityKind::PrivilegedHelper => match (state.privileged_helper_enabled, state.privileged_helper_path.as_deref()) {
            (true, Some(path)) if Path::new(path).exists() => (
                CapabilityState::Granted,
                format!("Privileged helper configured at {path}."),
            ),
            (true, Some(path)) => (
                CapabilityState::Unavailable,
                format!("Privileged helper is enabled but missing at {path}."),
            ),
            (true, None) => (
                CapabilityState::Unavailable,
                "Privileged helper is enabled but no helper path is configured.".to_owned(),
            ),
            _ => (
                CapabilityState::Unavailable,
                "Privileged helper is disabled. Enable it and provide a helper path for deeper attribution.".to_owned(),
            ),
        },
        _ => unreachable!("capability_status only handles adapter-backed capabilities"),
    }
}

fn docker_socket_path() -> String {
    env::var("AETOWER_DOCKER_SOCKET").unwrap_or_else(|_| "/var/run/docker.sock".to_owned())
}

fn fetch_chromium_targets(
    config: &ChromiumAdapterConfig,
    samples: &mut BTreeMap<String, ChromiumRuntimeSample>,
) -> Result<Vec<ChromiumPageTarget>, String> {
    let response = http_get_tcp(&config.host, config.port, &config.path, CHROMIUM_TIMEOUT)?;
    let parsed: Vec<ChromiumTarget> =
        serde_json::from_str(&response).map_err(|error| format!("invalid chromium json: {error}"))?;
    let captured_at_millis = crate::time::now_millis();
    let mut seen_ids = BTreeSet::new();

    let mut targets = parsed
        .into_iter()
        .filter(|target| target.target_type.as_deref() == Some("page"))
        .map(|target| ChromiumPageTarget {
            id: target.id.unwrap_or_else(|| "?".to_owned()),
            title: target
                .title
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Untitled page".to_owned()),
            url: target
                .url
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "about:blank".to_owned()),
            debug_socket: target.web_socket_debugger_url,
            js_heap_used_bytes: 0,
            js_heap_total_bytes: 0,
            dom_nodes: 0,
            documents: 0,
            frames: 0,
            cpu_percent: 0.0,
            network_bps: 0,
        })
        .map(|mut target| {
            seen_ids.insert(target.id.clone());
            if let Some(debug_socket) = target.debug_socket.as_deref() {
                if let Ok(metrics) = fetch_chromium_metrics(debug_socket) {
                    target.js_heap_used_bytes = metrics.js_heap_used_bytes;
                    target.js_heap_total_bytes = metrics.js_heap_total_bytes;
                    target.dom_nodes = metrics.dom_nodes;
                    target.documents = metrics.documents;
                    target.frames = metrics.frames;

                    if let Some(previous) = samples.insert(
                        target.id.clone(),
                        ChromiumRuntimeSample {
                            captured_at_millis,
                            task_duration_seconds: metrics.task_duration_seconds,
                            network_bytes: metrics.network_bytes,
                        },
                    ) {
                        let elapsed_seconds = ((captured_at_millis.saturating_sub(previous.captured_at_millis))
                            as f64
                            / 1000.0)
                            .max(0.001);
                        let task_delta = if metrics.task_duration_seconds >= previous.task_duration_seconds {
                            metrics.task_duration_seconds - previous.task_duration_seconds
                        } else {
                            0.0
                        };
                        let network_delta = metrics.network_bytes.saturating_sub(previous.network_bytes);
                        target.cpu_percent = ((task_delta / elapsed_seconds) * 100.0) as f32;
                        target.network_bps = ((network_delta as f64) / elapsed_seconds) as u64;
                    }
                }
            }
            target
        })
        .collect::<Vec<_>>();

    samples.retain(|id, _| seen_ids.contains(id));
    targets.sort_by(|left, right| right.cpu_percent.total_cmp(&left.cpu_percent).then(left.title.cmp(&right.title)));
    Ok(targets)
}

fn fetch_docker_containers(config: &DockerAdapterConfig) -> Result<Vec<DockerContainer>, String> {
    let response = http_get_unix(
        &config.socket_path,
        "/containers/json?all=1",
        DOCKER_TIMEOUT,
    )?;
    let parsed: Vec<DockerContainerSummary> =
        serde_json::from_str(&response).map_err(|error| format!("invalid docker json: {error}"))?;

    Ok(parsed
        .into_iter()
        .map(|container| DockerContainer {
            id: container.id.clone(),
            name: container
                .names
                .into_iter()
                .next()
                .unwrap_or_else(|| "/unnamed".to_owned())
                .trim_start_matches('/')
                .to_owned(),
            image: container.image,
            status: if container.status.is_empty() {
                container.state
            } else {
                container.status
            },
            ports: container
                .ports
                .into_iter()
                .map(|port| match (port.ip, port.public_port) {
                    (Some(ip), Some(public_port)) => {
                        format!("{}:{}->{} {}", ip, public_port, port.private_port, port.port_type)
                    }
                    (None, Some(public_port)) => {
                        format!("{}->{} {}", public_port, port.private_port, port.port_type)
                    }
                    _ => format!("{} {}", port.private_port, port.port_type),
                })
                .collect(),
            cpu_percent: 0.0,
            memory_usage_bytes: 0,
            memory_limit_bytes: 0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            block_read_bytes: 0,
            block_write_bytes: 0,
            pids: 0,
        })
        .map(|mut container| {
            if let Ok(stats) = fetch_docker_container_stats(config, &container.id) {
                container.cpu_percent = docker_cpu_percent(&stats);
                container.memory_usage_bytes = stats.memory_stats.usage.unwrap_or(0);
                container.memory_limit_bytes = stats.memory_stats.limit.unwrap_or(0);
                (container.network_rx_bytes, container.network_tx_bytes) =
                    docker_network_totals(&stats.networks);
                (container.block_read_bytes, container.block_write_bytes) =
                    docker_block_io_totals(&stats.blkio_stats.io_service_bytes_recursive);
                container.pids = stats.pids_stats.current.unwrap_or(0);
            }
            container
        })
        .collect())
}

fn fetch_privileged_helper_sample(helper_path: &str) -> Result<PrivilegedHelperSnapshot, String> {
    let output = Command::new(helper_path)
        .args(["sample", "--json"])
        .output()
        .map_err(|error| format!("helper execution failed: {error}"))?;
    if !output.status.success() {
        return Err(format!("helper exited with status {}", output.status));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid privileged helper json: {error}"))
}

fn helper_process_matches(entity: &EntitySnapshot, process: &PrivilegedProcessSample) -> bool {
    let display_name = entity.display_name.to_ascii_lowercase();
    let process_name = process.process_name.to_ascii_lowercase();
    let executable_name = entity
        .executable_path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let helper_executable = process
        .executable_name
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();

    display_name.contains(&process_name)
        || process_name.contains(&display_name)
        || (!helper_executable.is_empty() && executable_name == helper_executable)
}

fn parse_http_endpoint(raw: &str) -> Option<(String, u16, String)> {
    let without_scheme = raw.strip_prefix("http://")?;
    let (authority, path) = match without_scheme.split_once('/') {
        Some((authority, rest)) => (authority, format!("/{}", rest)),
        None => (without_scheme, "/json/list".to_owned()),
    };
    let (host, port) = authority.split_once(':')?;
    let port = port.parse().ok()?;
    Some((host.to_owned(), port, path))
}

fn fetch_chromium_metrics(debug_socket: &str) -> Result<ChromiumMetrics, String> {
    let _ = Url::parse(debug_socket).map_err(|error| format!("invalid websocket url: {error}"))?;
    let (mut socket, _) =
        connect(debug_socket).map_err(|error| format!("websocket connect failed: {error}"))?;

    let enable_command = json!({ "id": 1, "method": "Performance.enable" });
    socket
        .send(Message::Text(enable_command.to_string().into()))
        .map_err(|error| format!("websocket enable failed: {error}"))?;
    let _ = read_cdp_response(&mut socket, 1)?;

    let metrics_command = json!({ "id": 2, "method": "Performance.getMetrics" });
    socket
        .send(Message::Text(metrics_command.to_string().into()))
        .map_err(|error| format!("websocket metrics failed: {error}"))?;
    let payload = read_cdp_response(&mut socket, 2)?;
    let metrics = payload
        .get("result")
        .and_then(|value| value.get("metrics"))
        .and_then(Value::as_array)
        .ok_or_else(|| "cdp response missing metrics array".to_owned())?;

    let runtime_command = json!({
        "id": 3,
        "method": "Runtime.evaluate",
        "params": {
            "expression": "(()=>{const kinds=['navigation','resource'];let total=0;for(const kind of kinds){for(const entry of performance.getEntriesByType(kind)){total += entry.transferSize || entry.encodedBodySize || entry.decodedBodySize || 0;}}return Math.round(total);})()",
            "returnByValue": true
        }
    });
    socket
        .send(Message::Text(runtime_command.to_string().into()))
        .map_err(|error| format!("websocket runtime evaluate failed: {error}"))?;
    let runtime_payload = read_cdp_response(&mut socket, 3)?;

    let metric = |name: &str| -> u64 {
        metrics
            .iter()
            .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|value| value.get("value"))
            .and_then(Value::as_f64)
            .map(|value| value.max(0.0) as u64)
            .unwrap_or(0)
    };
    let metric_f64 = |name: &str| -> f64 {
        metrics
            .iter()
            .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|value| value.get("value"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let network_bytes = runtime_payload
        .get("result")
        .and_then(|value| value.get("result"))
        .and_then(|value| value.get("value"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    Ok(ChromiumMetrics {
        js_heap_used_bytes: metric("JSHeapUsedSize"),
        js_heap_total_bytes: metric("JSHeapTotalSize"),
        dom_nodes: metric("Nodes"),
        documents: metric("Documents"),
        frames: metric("Frames"),
        task_duration_seconds: metric_f64("TaskDuration"),
        network_bytes,
    })
}

fn read_cdp_response(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    expected_id: u64,
) -> Result<Value, String> {
    loop {
        let message = socket
            .read()
            .map_err(|error| format!("websocket read failed: {error}"))?;
        match message {
            Message::Text(text) => {
                let payload: Value = serde_json::from_str(&text)
                    .map_err(|error| format!("invalid cdp payload: {error}"))?;
                if payload.get("id").and_then(Value::as_u64) == Some(expected_id) {
                    return Ok(payload);
                }
            }
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
            Message::Close(_) => return Err("websocket closed before expected response".to_owned()),
        }
    }
}

fn fetch_docker_container_stats(
    config: &DockerAdapterConfig,
    container_id: &str,
) -> Result<DockerContainerStats, String> {
    let response = http_get_unix(
        &config.socket_path,
        &format!("/containers/{container_id}/stats?stream=0"),
        DOCKER_TIMEOUT,
    )?;
    serde_json::from_str(&response).map_err(|error| format!("invalid docker stats json: {error}"))
}

fn docker_cpu_percent(stats: &DockerContainerStats) -> f32 {
    let cpu_delta = stats
        .cpu_stats
        .cpu_usage
        .total_usage
        .saturating_sub(stats.pre_cpu_stats.cpu_usage.total_usage);
    let system_delta = stats
        .cpu_stats
        .system_cpu_usage
        .unwrap_or(0)
        .saturating_sub(stats.pre_cpu_stats.system_cpu_usage.unwrap_or(0));
    if cpu_delta == 0 || system_delta == 0 {
        return 0.0;
    }

    let cpu_count = stats
        .cpu_stats
        .online_cpus
        .filter(|count| *count > 0)
        .map(|count| count as f32)
        .unwrap_or_else(|| stats.cpu_stats.cpu_usage.percpu_usage.len().max(1) as f32);

    (cpu_delta as f32 / system_delta as f32) * cpu_count * 100.0
}

fn docker_network_totals(networks: &BTreeMap<String, DockerNetworkStats>) -> (u64, u64) {
    networks.values().fold((0, 0), |(rx, tx), network| {
        (
            rx.saturating_add(network.rx_bytes),
            tx.saturating_add(network.tx_bytes),
        )
    })
}

fn docker_block_io_totals(entries: &[DockerBlkioEntry]) -> (u64, u64) {
    entries.iter().fold((0, 0), |(read, write), entry| {
        let value = entry.value.unwrap_or(0);
        match entry.op.as_deref() {
            Some("Read") => (read.saturating_add(value), write),
            Some("Write") => (read, write.saturating_add(value)),
            _ => (read, write),
        }
    })
}

fn chromium_target_detail(target: &ChromiumPageTarget) -> String {
    let mut parts = vec![target.url.clone()];
    if target.js_heap_used_bytes > 0 || target.js_heap_total_bytes > 0 {
        parts.push(format!(
            "heap {} / {}",
            human_bytes(target.js_heap_used_bytes),
            human_bytes(target.js_heap_total_bytes)
        ));
    }
    if target.network_bps > 0 {
        parts.push(format!("net {}/s", human_bytes(target.network_bps)));
    }
    if target.dom_nodes > 0 || target.documents > 0 || target.frames > 0 {
        parts.push(format!(
            "{} nodes · {} docs · {} frames",
            target.dom_nodes, target.documents, target.frames
        ));
    }
    if let Some(socket) = target.debug_socket.as_deref() {
        parts.push(socket.to_owned());
    }
    parts.join(" · ")
}

fn docker_container_detail(container: &DockerContainer) -> String {
    let mut parts = vec![
        container.image.clone(),
        container.status.clone(),
        format!(
            "cpu {:.1}% · mem {}",
            container.cpu_percent,
            human_bytes(container.memory_usage_bytes)
        ),
    ];
    if container.memory_limit_bytes > 0 {
        parts.push(format!("limit {}", human_bytes(container.memory_limit_bytes)));
    }
    if container.network_rx_bytes > 0 || container.network_tx_bytes > 0 {
        parts.push(format!(
            "net {}↓ {}↑",
            human_bytes(container.network_rx_bytes),
            human_bytes(container.network_tx_bytes)
        ));
    }
    if container.block_read_bytes > 0 || container.block_write_bytes > 0 {
        parts.push(format!(
            "io {} read {} write",
            human_bytes(container.block_read_bytes),
            human_bytes(container.block_write_bytes)
        ));
    }
    if container.pids > 0 {
        parts.push(format!("{} pids", container.pids));
    }
    if !container.ports.is_empty() {
        parts.push(container.ports.join(", "));
    }
    parts.join(" · ")
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GiB", value / GB)
    } else if value >= MB {
        format!("{:.1} MiB", value / MB)
    } else if value >= KB {
        format!("{:.1} KiB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

fn http_get_tcp(host: &str, port: u16, path: &str, timeout: Duration) -> Result<String, String> {
    let mut stream =
        TcpStream::connect((host, port)).map_err(|error| format!("tcp connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("tcp read timeout failed: {error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("tcp write timeout failed: {error}"))?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        path, host, port
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("tcp write failed: {error}"))?;

    read_http_body(&mut stream)
}

fn http_get_unix(socket_path: &str, path: &str, timeout: Duration) -> Result<String, String> {
    let mut stream =
        UnixStream::connect(socket_path).map_err(|error| format!("unix connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("unix read timeout failed: {error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("unix write timeout failed: {error}"))?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n",
        path
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("unix write failed: {error}"))?;

    read_http_body(&mut stream)
}

fn read_http_body(stream: &mut impl Read) -> Result<String, String> {
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read failed: {error}"))?;

    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "http response missing header terminator".to_owned())?;
    Ok(body.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        capability_status, docker_block_io_totals, docker_cpu_percent, docker_network_totals,
        parse_http_endpoint, AdapterState, CapabilityKind, CapabilityState, ChromiumTarget,
        DockerBlkioEntry, DockerContainerStats, DockerContainerSummary, DockerNetworkStats,
    };

    #[test]
    fn parses_http_endpoint_with_explicit_path() {
        let result = parse_http_endpoint("http://127.0.0.1:9222/json/list");
        assert_eq!(
            result,
            Some(("127.0.0.1".to_owned(), 9222, "/json/list".to_owned()))
        );
    }

    #[test]
    fn parses_http_endpoint_with_default_path() {
        let result = parse_http_endpoint("http://localhost:9223");
        assert_eq!(
            result,
            Some(("localhost".to_owned(), 9223, "/json/list".to_owned()))
        );
    }

    #[test]
    fn chromium_target_deserializes() {
        let raw = r#"{"type":"page","id":"1","title":"Docs","url":"https://example.com","webSocketDebuggerUrl":"ws://localhost/devtools/page/1"}"#;
        let parsed: ChromiumTarget = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.target_type.as_deref(), Some("page"));
        assert_eq!(parsed.id.as_deref(), Some("1"));
        assert_eq!(parsed.title.as_deref(), Some("Docs"));
    }

    #[test]
    fn docker_container_deserializes() {
        let raw = r#"{"Id":"abc123","Names":["/web"],"Image":"nginx:latest","State":"running","Status":"Up 3 minutes","Ports":[{"IP":"0.0.0.0","PrivatePort":80,"PublicPort":8080,"Type":"tcp"}]}"#;
        let parsed: DockerContainerSummary = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.names[0], "/web");
        assert_eq!(parsed.image, "nginx:latest");
        assert_eq!(parsed.status, "Up 3 minutes");
        assert_eq!(parsed.ports[0].public_port, Some(8080));
    }

    #[test]
    fn docker_cpu_percent_uses_usage_deltas() {
        let raw = r#"{
            "cpu_stats":{"cpu_usage":{"total_usage":300,"percpu_usage":[100,200]},"system_cpu_usage":2000,"online_cpus":2},
            "precpu_stats":{"cpu_usage":{"total_usage":100,"percpu_usage":[50,50]},"system_cpu_usage":1000,"online_cpus":2},
            "memory_stats":{"usage":0,"limit":0},
            "networks":{},
            "blkio_stats":{"io_service_bytes_recursive":[]},
            "pids_stats":{"current":1}
        }"#;
        let parsed: DockerContainerStats = serde_json::from_str(raw).unwrap();
        assert_eq!(docker_cpu_percent(&parsed), 40.0);
    }

    #[test]
    fn docker_network_totals_accumulate_interfaces() {
        let mut networks = BTreeMap::new();
        networks.insert(
            "eth0".to_owned(),
            DockerNetworkStats {
                rx_bytes: 1024,
                tx_bytes: 2048,
            },
        );
        networks.insert(
            "eth1".to_owned(),
            DockerNetworkStats {
                rx_bytes: 512,
                tx_bytes: 256,
            },
        );
        assert_eq!(docker_network_totals(&networks), (1536, 2304));
    }

    #[test]
    fn docker_block_io_totals_split_read_write() {
        let entries = vec![
            DockerBlkioEntry {
                op: Some("Read".to_owned()),
                value: Some(2048),
            },
            DockerBlkioEntry {
                op: Some("Write".to_owned()),
                value: Some(4096),
            },
            DockerBlkioEntry {
                op: Some("Read".to_owned()),
                value: Some(1024),
            },
        ];
        assert_eq!(docker_block_io_totals(&entries), (3072, 4096));
    }

    #[test]
    fn privileged_helper_capability_reflects_configuration() {
        let state = AdapterState {
            privileged_helper_enabled: true,
            privileged_helper_path: Some("/tmp/missing-helper".to_owned()),
            ..AdapterState::default()
        };
        let (capability_state, detail) = capability_status(&state, &CapabilityKind::PrivilegedHelper);
        assert_eq!(capability_state, CapabilityState::Unavailable);
        assert!(detail.contains("missing"));
    }
}

use std::{
    collections::BTreeMap,
    env,
    io::{Read, Write},
    net::TcpStream,
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

use aetower_model::{
    CapabilityKind, CapabilitySnapshot, CapabilityState, ComponentKind, ComponentSnapshot,
    EntitySnapshot,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tungstenite::{connect, Message};
use url::Url;

const CHROMIUM_TIMEOUT: Duration = Duration::from_millis(300);
const DOCKER_TIMEOUT: Duration = Duration::from_millis(300);

#[derive(Default)]
pub struct AdapterManager;

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

impl AdapterManager {
    pub fn initial_capabilities() -> BTreeMap<CapabilityKind, CapabilitySnapshot> {
        let now = crate::time::now_millis();
        let docker_socket_path = docker_socket_path();
        let docker_available = Path::new(&docker_socket_path).exists();
        let chromium_config = chromium_config();

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
                    state: if chromium_config.is_some() {
                        CapabilityState::Granted
                    } else {
                        CapabilityState::Unavailable
                    },
                    detail: chromium_config
                        .map(|config| {
                            format!(
                                "Chromium target discovery configured at {}:{}{}.",
                                config.host, config.port, config.path
                            )
                        })
                        .unwrap_or_else(|| {
                            "Set AETOWER_CHROMIUM_ENDPOINT to a local /json or /json/list endpoint.".to_owned()
                        }),
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
                        format!("Docker socket detected at {}.", docker_socket_path)
                    } else {
                        format!("Docker socket not detected at {}.", docker_socket_path)
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
        let chromium_targets = capabilities
            .get(&CapabilityKind::ChromiumDebug)
            .filter(|capability| capability.state == CapabilityState::Granted)
            .and_then(|_| chromium_config())
            .and_then(|config| fetch_chromium_targets(&config).ok());

        let docker_containers = capabilities
            .get(&CapabilityKind::DockerSocket)
            .filter(|capability| capability.state == CapabilityState::Granted)
            .map(|_| DockerAdapterConfig {
                socket_path: docker_socket_path(),
            })
            .and_then(|config| fetch_docker_containers(&config).ok());

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
                            cpu_percent: 0.0,
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

fn chromium_config() -> Option<ChromiumAdapterConfig> {
    let raw = env::var("AETOWER_CHROMIUM_ENDPOINT").ok()?;
    parse_http_endpoint(&raw).map(|(host, port, path)| ChromiumAdapterConfig { host, port, path })
}

fn docker_socket_path() -> String {
    env::var("AETOWER_DOCKER_SOCKET").unwrap_or_else(|_| "/var/run/docker.sock".to_owned())
}

fn fetch_chromium_targets(config: &ChromiumAdapterConfig) -> Result<Vec<ChromiumPageTarget>, String> {
    let response = http_get_tcp(&config.host, config.port, &config.path, CHROMIUM_TIMEOUT)?;
    let parsed: Vec<ChromiumTarget> =
        serde_json::from_str(&response).map_err(|error| format!("invalid chromium json: {error}"))?;

    Ok(parsed
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
        })
        .map(|mut target| {
            if let Some(debug_socket) = target.debug_socket.as_deref() {
                if let Ok(metrics) = fetch_chromium_metrics(debug_socket) {
                    target.js_heap_used_bytes = metrics.js_heap_used_bytes;
                    target.js_heap_total_bytes = metrics.js_heap_total_bytes;
                    target.dom_nodes = metrics.dom_nodes;
                    target.documents = metrics.documents;
                    target.frames = metrics.frames;
                }
            }
            target
        })
        .collect())
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

#[derive(Debug, Default)]
struct ChromiumMetrics {
    js_heap_used_bytes: u64,
    js_heap_total_bytes: u64,
    dom_nodes: u64,
    documents: u64,
    frames: u64,
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

    let metric = |name: &str| -> u64 {
        metrics
            .iter()
            .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|value| value.get("value"))
            .and_then(Value::as_f64)
            .map(|value| value.max(0.0) as u64)
            .unwrap_or(0)
    };

    Ok(ChromiumMetrics {
        js_heap_used_bytes: metric("JSHeapUsedSize"),
        js_heap_total_bytes: metric("JSHeapTotalSize"),
        dom_nodes: metric("Nodes"),
        documents: metric("Documents"),
        frames: metric("Frames"),
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
        docker_block_io_totals, docker_cpu_percent, docker_network_totals, parse_http_endpoint,
        ChromiumTarget, DockerBlkioEntry, DockerContainerStats, DockerContainerSummary,
        DockerNetworkStats,
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
}

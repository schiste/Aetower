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
                            detail: match target.debug_socket.as_deref() {
                                Some(socket) => format!("{} · {}", target.url, socket),
                                None => target.url.clone(),
                            },
                            cpu_percent: 0.0,
                            memory_bytes: 0,
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
                            detail: if container.ports.is_empty() {
                                format!("{} · {}", container.image, container.status)
                            } else {
                                format!(
                                    "{} · {} · {}",
                                    container.image,
                                    container.status,
                                    container.ports.join(", ")
                                )
                            },
                            cpu_percent: 0.0,
                            memory_bytes: 0,
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
}

#[derive(Debug, Clone)]
struct DockerContainer {
    name: String,
    image: String,
    status: String,
    ports: Vec<String>,
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
        })
        .collect())
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
    use super::{parse_http_endpoint, ChromiumTarget, DockerContainerSummary};

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
        let raw = r#"{"Names":["/web"],"Image":"nginx:latest","State":"running","Status":"Up 3 minutes","Ports":[{"IP":"0.0.0.0","PrivatePort":80,"PublicPort":8080,"Type":"tcp"}]}"#;
        let parsed: DockerContainerSummary = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.names[0], "/web");
        assert_eq!(parsed.image, "nginx:latest");
        assert_eq!(parsed.status, "Up 3 minutes");
        assert_eq!(parsed.ports[0].public_port, Some(8080));
    }
}

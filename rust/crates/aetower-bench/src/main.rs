use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use aetower_core::{BenchmarkConfig, run_benchmark};
use aetower_model::{
    AggregateMetrics, EntitySnapshot, FrictionBreakdown, HostSnapshot, RuntimeLagMetrics,
    SystemSnapshot, ThermalState,
};
use aetower_telemetry::{OtlpConfig, TelemetryExporter};
use serde_json::json;

fn main() -> Result<(), String> {
    match parse_command(std::env::args().skip(1))? {
        Command::Benchmark(config) => run_benchmark_command(config),
        Command::VerifyTelemetryLoopback => run_telemetry_smoke(),
    }
}

enum Command {
    Benchmark(BenchmarkConfig),
    VerifyTelemetryLoopback,
}

fn run_benchmark_command(config: BenchmarkConfig) -> Result<(), String> {
    let enforce_default_budget = config.enforce_default_budget;
    let report = run_benchmark(config);
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("json encode failed: {error}"))?;
    println!("{json}");
    if enforce_default_budget && !report.default_budget_result.passed {
        return Err(format!(
            "benchmark exceeded default budget: {}",
            report.default_budget_result.violations.join("; ")
        ));
    }
    Ok(())
}

fn run_telemetry_smoke() -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("telemetry smoke bind failed: {error}"))?;
    let listen_addr = listener
        .local_addr()
        .map_err(|error| format!("telemetry smoke local addr failed: {error}"))?;

    let collector = thread::spawn(move || run_loopback_collector(listener, listen_addr));
    let endpoint = format!("http://{listen_addr}/v1/metrics");
    let exporter = TelemetryExporter::new(OtlpConfig {
        endpoint: endpoint.clone(),
        export_interval_secs: 30,
        enabled: false,
    });

    exporter.verify_export(&synthetic_snapshot(), &synthetic_lag_metrics())?;
    let receipt = collector
        .join()
        .map_err(|_| "telemetry smoke collector thread panicked".to_owned())??;

    let summary = json!({
        "endpoint": endpoint,
        "requestPath": receipt.path,
        "contentType": receipt.content_type,
        "payloadBytes": receipt.payload_bytes,
        "resourceMetricCount": receipt.resource_metric_count,
        "scopeMetricCount": receipt.scope_metric_count,
        "metricCount": receipt.metric_count,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .map_err(|error| format!("telemetry smoke summary encode failed: {error}"))?
    );
    Ok(())
}

fn parse_command(args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut config = BenchmarkConfig::default();
    let mut verify_telemetry_loopback = false;
    let mut args = args.peekable();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--iterations" | "-n" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --iterations".to_owned())?;
                config.iterations = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid iteration count '{value}': {error}"))?;
            }
            "--warmup" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --warmup".to_owned())?;
                config.warmup_iterations = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid warmup count '{value}': {error}"))?;
            }
            "--enforce" | "--enforce-default-budget" => {
                config.enforce_default_budget = true;
            }
            "--verify-telemetry-loopback" => {
                verify_telemetry_loopback = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => {
                return Err(format!("unsupported argument: {other}"));
            }
        }
    }

    if verify_telemetry_loopback {
        return Ok(Command::VerifyTelemetryLoopback);
    }

    config.iterations = config.iterations.max(1);
    Ok(Command::Benchmark(config))
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p aetower-bench --release -- [--iterations <count>] [--enforce]\n\
         Optional: [--warmup <count>] [--verify-telemetry-loopback]\n\
         Runs the core collection pipeline repeatedly, or starts a loopback OTLP smoke receiver and verifies telemetry export end to end."
    );
}

#[derive(Debug)]
struct CollectorReceipt {
    path: String,
    content_type: String,
    payload_bytes: usize,
    resource_metric_count: usize,
    scope_metric_count: usize,
    metric_count: usize,
}

fn run_loopback_collector(
    listener: TcpListener,
    address: SocketAddr,
) -> Result<CollectorReceipt, String> {
    listener
        .set_nonblocking(false)
        .map_err(|error| format!("telemetry smoke listener setup failed: {error}"))?;
    let (mut stream, _) = listener
        .accept()
        .map_err(|error| format!("telemetry smoke accept failed on {address}: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("telemetry smoke read timeout failed: {error}"))?;

    let mut buffer = Vec::with_capacity(16 * 1024);
    let header_end = loop {
        let mut chunk = [0u8; 4096];
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("telemetry smoke read failed: {error}"))?;
        if read == 0 {
            return Err("telemetry smoke collector received an empty request".to_owned());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
        if buffer.len() > 512 * 1024 {
            return Err("telemetry smoke collector request headers exceeded 512 KiB".to_owned());
        }
    };

    let headers = std::str::from_utf8(&buffer[..header_end])
        .map_err(|error| format!("telemetry smoke invalid request headers: {error}"))?;
    let mut lines = headers.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| "telemetry smoke request line missing".to_owned())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let path = request_parts.next().unwrap_or_default().to_owned();
    if method != "POST" {
        return Err(format!(
            "telemetry smoke expected POST request, got {method}"
        ));
    }

    let mut content_length = None;
    let mut content_type = String::new();
    for line in lines {
        let Some((raw_name, raw_value)) = line.split_once(':') else {
            continue;
        };
        let name = raw_name.trim().to_ascii_lowercase();
        let value = raw_value.trim();
        match name.as_str() {
            "content-length" => {
                content_length = Some(value.parse::<usize>().map_err(|error| {
                    format!("telemetry smoke invalid content length '{value}': {error}")
                })?);
            }
            "content-type" => {
                content_type = value.to_owned();
            }
            _ => {}
        }
    }
    let content_length =
        content_length.ok_or_else(|| "telemetry smoke content length missing".to_owned())?;
    let body_start = header_end + 4;
    while buffer.len().saturating_sub(body_start) < content_length {
        let mut chunk = [0u8; 4096];
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("telemetry smoke body read failed: {error}"))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = buffer
        .get(body_start..body_start.saturating_add(content_length))
        .ok_or_else(|| "telemetry smoke request body was truncated".to_owned())?;

    let payload: serde_json::Value = serde_json::from_slice(body)
        .map_err(|error| format!("telemetry smoke invalid JSON body: {error}"))?;
    let resource_metrics = payload
        .get("resourceMetrics")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "telemetry smoke payload missing resourceMetrics".to_owned())?;
    let scope_metric_count = resource_metrics
        .iter()
        .map(|resource| {
            resource
                .get("scopeMetrics")
                .and_then(|value| value.as_array())
                .map(|scopes| scopes.len())
                .unwrap_or(0)
        })
        .sum::<usize>();
    let metric_count = resource_metrics
        .iter()
        .flat_map(|resource| {
            resource
                .get("scopeMetrics")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
        })
        .map(|scope| {
            scope
                .get("metrics")
                .and_then(|value| value.as_array())
                .map(|metrics| metrics.len())
                .unwrap_or(0)
        })
        .sum::<usize>();

    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}")
        .map_err(|error| format!("telemetry smoke response write failed: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("telemetry smoke response flush failed: {error}"))?;

    if resource_metrics.is_empty() || metric_count == 0 {
        return Err("telemetry smoke collector did not receive any metrics".to_owned());
    }

    Ok(CollectorReceipt {
        path,
        content_type,
        payload_bytes: body.len(),
        resource_metric_count: resource_metrics.len(),
        scope_metric_count,
        metric_count,
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn synthetic_snapshot() -> SystemSnapshot {
    let now_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);
    SystemSnapshot {
        sequence: 1,
        captured_at_millis: now_millis,
        host: HostSnapshot {
            cpu_percent: 17.5,
            memory_used_bytes: 6 * 1024 * 1024 * 1024,
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            network_receive_bps: 128_000,
            network_send_bps: 64_000,
            wakeups_per_second: 42.0,
            thermal_state: ThermalState::Fair,
            gpu_percent: 23.0,
            ane_percent: 5.0,
            ..HostSnapshot::default()
        },
        entities: vec![EntitySnapshot {
            entity_id: "telemetry-smoke-entity".to_owned(),
            display_name: "Telemetry Smoke Entity".to_owned(),
            metrics: AggregateMetrics {
                cpu_percent: 12.0,
                memory_resident_bytes: 256 * 1024 * 1024,
                network_receive_bps: 96_000,
                network_send_bps: 32_000,
                wakeups_per_second: 18.0,
                process_count: 3,
                ..AggregateMetrics::default()
            },
            friction: FrictionBreakdown {
                total_score: 41.0,
                energy_impact_score: 12.0,
                ..FrictionBreakdown::default()
            },
            ..EntitySnapshot::default()
        }],
        ..SystemSnapshot::default()
    }
}

fn synthetic_lag_metrics() -> RuntimeLagMetrics {
    RuntimeLagMetrics {
        updated_at_millis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0),
        engine_tick_millis: 4.2,
        collect_millis: 1.3,
        identity_millis: 0.7,
        attribution_millis: 0.4,
        friction_millis: 0.2,
        bridge_fetch_millis: 0.8,
        ui_refresh_millis: 1.1,
        snapshot_to_ui_millis: 6.0,
        snapshot_to_render_millis: 8.4,
        display_frame_interval_millis: 8.3,
        display_refresh_hz: 120.0,
        display_dropped_frames: 0,
        input_avg_latency_millis: 1.7,
        input_max_latency_millis: 2.4,
        input_sample_count: 12,
        ..RuntimeLagMetrics::default()
    }
}

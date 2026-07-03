//! `aetower` — the operator CLI.
//!
//! A thin second client of the same local MCP engine the app and the agents
//! talk to. Every read verb resolves the app's Unix socket, calls one tool, and
//! renders it; `--json` on any verb emits the raw structured payload for
//! `| jq` pipelines. The CLI needs the app running (the socket is app-owned) —
//! when it isn't, verbs print a "launch Aetower" hint and exit `3`.

mod client;
mod install;
mod table;
mod verbs;

use std::io;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use clap::{CommandFactory, Parser, Subcommand};
use serde_json::{Map, Value};

use client::{Client, exit};

#[derive(Parser)]
#[command(
    name = "aetower",
    version,
    about = "Operator console for the machine your agents run on — live data in your shell.",
    long_about = "aetower talks to a running Aetower.app over its local MCP socket and prints \
                  live system state: friction, energy, storage, and repositories. Requires the \
                  app to be running. Add --json to any command for machine-readable output."
)]
struct Cli {
    /// Emit the raw structured JSON payload instead of a formatted view.
    #[arg(long, global = true)]
    json: bool,

    /// Override the MCP socket path (default: ~/.aetower/mcp.sock).
    #[arg(long, value_name = "PATH", global = true)]
    socket: Option<std::path::PathBuf>,

    /// Refresh every SECS seconds until interrupted (read verbs only).
    #[arg(long, value_name = "SECS", global = true)]
    watch: Option<u64>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Loudest entities right now (friction, CPU, memory).
    Top {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// The recommendation feed — what's straining the machine and what to do.
    Findings,
    /// Active alerts and budget breaches.
    Alerts {
        /// Exit non-zero if any alert is at warning severity or above.
        #[arg(long)]
        fail_on_warn: bool,
    },
    /// Host summary: CPU, memory, wakeups, energy, thermal.
    Host,
    /// Storage pressure and reclaim opportunities.
    Storage,
    /// Repository health: git state, agent-contract readiness, clones.
    Repos {
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },
    /// Self-check: reachability, capabilities, and engine health.
    Doctor,
    /// List every tool the running app exposes.
    Tools,
    /// Call any MCP tool directly and print its JSON payload.
    Call {
        /// Tool name, e.g. aetower_top_findings.
        tool: String,
        /// Tool argument as key=value (repeatable). Values are parsed as JSON
        /// when possible, otherwise treated as a string.
        #[arg(long = "arg", value_name = "K=V")]
        args: Vec<String>,
    },
    /// Install the `aetower` command onto your PATH (/usr/local/bin symlink).
    Install,
    /// Remove the `aetower` PATH symlink.
    Uninstall,
    /// Print a shell completion script (zsh, bash, fish, …).
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Print the man page (roff) — pipe to a file under man/man1.
    Man,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    ExitCode::from(run(cli) as u8)
}

fn run(cli: Cli) -> i32 {
    let client = Client::new(cli.socket.clone());

    match cli.command {
        Command::Top { limit } => display(
            &client,
            cli.json,
            cli.watch,
            "aetower_host_summary",
            json_args(&[]),
            |v| verbs::top(v, limit),
        ),
        Command::Findings => display(
            &client,
            cli.json,
            cli.watch,
            "aetower_top_findings",
            json_args(&[]),
            verbs::findings,
        ),
        Command::Host => display(
            &client,
            cli.json,
            cli.watch,
            "aetower_host_summary",
            json_args(&[]),
            verbs::host,
        ),
        Command::Storage => display(
            &client,
            cli.json,
            cli.watch,
            "aetower_storage_hygiene_overview",
            json_args(&[]),
            verbs::storage,
        ),
        Command::Repos { limit } => display(
            &client,
            cli.json,
            cli.watch,
            "aetower_repository_inventory",
            json_args(&[]),
            |v| verbs::repos(v, limit),
        ),
        Command::Alerts { fail_on_warn } => run_alerts(&client, cli.json, cli.watch, fail_on_warn),
        Command::Doctor => run_doctor(&client, cli.json),
        Command::Tools => run_tools(&client, cli.json),
        Command::Call { tool, args } => run_call(&client, cli.json, &tool, &args),
        Command::Install => local_result(install::install()),
        Command::Uninstall => local_result(install::uninstall()),
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "aetower", &mut io::stdout());
            exit::OK
        }
        Command::Man => match clap_mangen::Man::new(Cli::command()).render(&mut io::stdout()) {
            Ok(()) => exit::OK,
            Err(error) => {
                eprintln!("aetower: render man page: {error}");
                exit::LOCAL
            }
        },
    }
}

/// Build a tool `arguments` object from `(key, value)` pairs.
fn json_args(pairs: &[(&str, Value)]) -> Value {
    let mut map = Map::new();
    for (k, v) in pairs {
        map.insert((*k).to_string(), v.clone());
    }
    Value::Object(map)
}

/// Shared render loop for the read verbs, with `--watch` and `--json` support.
fn display<F>(
    client: &Client,
    as_json: bool,
    watch: Option<u64>,
    tool: &str,
    args: Value,
    fmt: F,
) -> i32
where
    F: Fn(&Value) -> String,
{
    loop {
        if !client.is_reachable() {
            eprintln!("{}", client.unreachable_message());
            return exit::UNREACHABLE;
        }
        match client.fetch(tool, args.clone()) {
            Ok(value) => {
                if as_json {
                    println!("{}", compact(&value));
                } else {
                    if watch.is_some() {
                        // Clear screen + home cursor for a stable watch view.
                        print!("\x1b[2J\x1b[H");
                    }
                    println!("{}", fmt(&value));
                }
            }
            Err(error) => {
                eprintln!("aetower: {error}");
                return exit::TOOL_ERROR;
            }
        }
        match watch {
            Some(secs) => thread::sleep(Duration::from_secs(secs.max(1))),
            None => return exit::OK,
        }
    }
}

fn run_alerts(client: &Client, as_json: bool, watch: Option<u64>, fail_on_warn: bool) -> i32 {
    // --fail-on-warn is a one-shot CI gate; ignore --watch when it's set.
    if fail_on_warn {
        if !client.is_reachable() {
            eprintln!("{}", client.unreachable_message());
            return exit::UNREACHABLE;
        }
        return match client.fetch("aetower_host_alerts", json_args(&[])) {
            Ok(value) => {
                if as_json {
                    println!("{}", compact(&value));
                } else {
                    println!("{}", verbs::alerts(&value));
                }
                if verbs::alerts_breached(&value) {
                    exit::ALERT
                } else {
                    exit::OK
                }
            }
            Err(error) => {
                eprintln!("aetower: {error}");
                exit::TOOL_ERROR
            }
        };
    }
    display(
        client,
        as_json,
        watch,
        "aetower_host_alerts",
        json_args(&[]),
        verbs::alerts,
    )
}

fn run_doctor(client: &Client, as_json: bool) -> i32 {
    if !client.is_reachable() {
        // Doctor still reports the failure clearly, then exits unreachable.
        eprintln!("{}", client.unreachable_message());
        return exit::UNREACHABLE;
    }
    let host = match client.fetch("aetower_host_summary", json_args(&[])) {
        Ok(v) => v,
        Err(error) => {
            eprintln!("aetower: {error}");
            return exit::TOOL_ERROR;
        }
    };
    let diagnostics = client
        .fetch("aetower_diagnostics_overview", json_args(&[]))
        .unwrap_or(Value::Null);

    if as_json {
        println!(
            "{}",
            compact(&json_args(&[
                ("host_summary", host.clone()),
                ("diagnostics", diagnostics.clone()),
            ]))
        );
    } else {
        println!(
            "{}",
            verbs::doctor(
                &host,
                &diagnostics,
                &client.socket_path().display().to_string()
            )
        );
    }
    if verbs::doctor_unhealthy(&diagnostics) {
        exit::TOOL_ERROR
    } else {
        exit::OK
    }
}

fn run_tools(client: &Client, as_json: bool) -> i32 {
    if !client.is_reachable() {
        eprintln!("{}", client.unreachable_message());
        return exit::UNREACHABLE;
    }
    match client.tools() {
        Ok(value) => {
            if as_json {
                println!("{}", compact(&value));
            } else {
                println!("{}", verbs::tools(&value));
            }
            exit::OK
        }
        Err(error) => {
            eprintln!("aetower: {error}");
            exit::TOOL_ERROR
        }
    }
}

fn run_call(client: &Client, as_json: bool, tool: &str, args: &[String]) -> i32 {
    if !client.is_reachable() {
        eprintln!("{}", client.unreachable_message());
        return exit::UNREACHABLE;
    }
    let mut map = Map::new();
    for pair in args {
        let Some((key, raw)) = pair.split_once('=') else {
            eprintln!("aetower: --arg must be key=value, got '{pair}'");
            return exit::LOCAL;
        };
        // Parse the value as JSON (number/bool/array/object) when it parses,
        // otherwise keep it as a plain string.
        let value =
            serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
        map.insert(key.to_string(), value);
    }
    match client.fetch(tool, Value::Object(map)) {
        Ok(value) => {
            if as_json {
                println!("{}", compact(&value));
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| compact(&value))
                );
            }
            exit::OK
        }
        Err(error) => {
            eprintln!("aetower: {error}");
            exit::TOOL_ERROR
        }
    }
}

fn local_result(result: Result<String, String>) -> i32 {
    match result {
        Ok(message) => {
            println!("{message}");
            exit::OK
        }
        Err(error) => {
            eprintln!("aetower: {error}");
            exit::LOCAL
        }
    }
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

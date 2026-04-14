use std::path::PathBuf;
use std::process::ExitCode;

use aetower_mcp::proxy_stdio_to_socket;

const USAGE: &str = "usage: aetower-mcp <socket-path>\n\n\
     Bridges JSON-RPC over stdio to the Unix socket served by a running Aetower.app.\n\
     The socket path is typically ~/.aetower/mcp.sock and is owned by the app's\n\
     in-process MCP server; when Aetower.app exits, this helper's connection closes\n\
     and the process terminates naturally.\n";

fn main() -> ExitCode {
    install_panic_hook();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 || matches!(args[0].as_str(), "-h" | "--help") {
        eprint!("{USAGE}");
        return if args.len() == 1 && matches!(args[0].as_str(), "-h" | "--help") {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(64) // EX_USAGE
        };
    }

    let socket_path = PathBuf::from(&args[0]);
    match proxy_stdio_to_socket(&socket_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("aetower-mcp: {error}");
            if looks_like_unreachable(&error) {
                eprintln!(
                    "aetower-mcp: the Aetower.app MCP socket at {} is not reachable.\n\
                     Start Aetower.app to enable these tools.",
                    socket_path.display()
                );
            }
            ExitCode::from(69) // EX_UNAVAILABLE — socket unreachable / peer gone
        }
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("aetower-mcp: proxy panicked: {info}");
        previous(info);
    }));
}

fn looks_like_unreachable(error: &str) -> bool {
    // proxy_streams_to_socket prefixes connect failures with "connect MCP socket"
    // — a stable marker we can key off without parsing errno text.
    error.contains("connect MCP socket")
}

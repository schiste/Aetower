use std::path::PathBuf;
use std::process::ExitCode;

use aetower_mcp::proxy_stdio_to_socket;

const USAGE: &str = "usage: aetower-mcp <socket-path>\n\n\
     Bridges JSON-RPC over stdio to the Unix socket served by a running Aetower.app.\n\
     The socket path is typically ~/.aetower/mcp.sock and is owned by the app's\n\
     in-process MCP server; when Aetower.app exits, this helper's connection closes\n\
     and the process terminates naturally.\n";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 || matches!(args[0].as_str(), "-h" | "--help") {
        eprint!("{USAGE}");
        return if args.len() == 1 && matches!(args[0].as_str(), "-h" | "--help") {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(64) // EX_USAGE
        };
    }

    match proxy_stdio_to_socket(PathBuf::from(&args[0])) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("aetower-mcp: {error}");
            ExitCode::from(69) // EX_UNAVAILABLE — socket unreachable / peer gone
        }
    }
}

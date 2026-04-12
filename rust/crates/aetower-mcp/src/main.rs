use aetower_mcp::{default_socket_path, proxy_stdio_to_socket};

fn main() -> Result<(), String> {
    let socket_path = match std::env::args().nth(1) {
        Some(path) => std::path::PathBuf::from(path),
        None => default_socket_path(),
    };
    proxy_stdio_to_socket(socket_path)
}

//! JSON-RPC / Unix-socket transport for the Aetower MCP server.
//!
//! Owns the bits that move bytes between Aetower and an MCP client:
//!
//! * The Unix-socket listener and its connection handler
//! * The stdio-to-socket proxy used by the helper binary
//! * Content-Length and JSON-line message framing
//! * The shared runtime-stats counters exposed via the `LocalMcpServerHandle`
//!
//! The actual tool dispatch lives in the parent module (`AetowerMcpServer`);
//! this module exposes only what dispatch needs to talk over the wire.

use std::{
    env, fs,
    io::{BufReader, Read, Write},
    net::Shutdown,
    os::fd::AsRawFd,
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use serde_json::{Value, json};

use crate::{AetowerMcpDataSource, AetowerMcpServer, DynamicExecutionMode};

pub(crate) const SOCKET_DIR_MODE: u32 = 0o700;
pub(crate) const SOCKET_FILE_MODE: u32 = 0o600;
const MCP_READ_TIMEOUT: Duration = Duration::from_millis(250);
const PROXY_POLL_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageFraming {
    ContentLength,
    JsonLine,
}

pub(crate) enum ReadMessageOutcome {
    Message(Value),
    EndOfStream,
    Timeout,
}

pub struct LocalMcpServerHandle {
    running: Arc<AtomicBool>,
    join_handle: Option<thread::JoinHandle<()>>,
    client_threads: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
    socket_path: PathBuf,
    stats: Arc<McpRuntimeStats>,
}

impl LocalMcpServerHandle {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn stats(&self) -> (u64, u64, u64) {
        self.stats.snapshot()
    }
}

impl Drop for LocalMcpServerHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.socket_path);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
        if let Ok(mut client_threads) = self.client_threads.lock() {
            for join_handle in client_threads.drain(..) {
                let _ = join_handle.join();
            }
        }
        let _ = fs::remove_file(&self.socket_path);
    }
}

#[derive(Default)]
pub(crate) struct McpRuntimeStats {
    pub(crate) total_connections: AtomicU64,
    pub(crate) active_clients: AtomicUsize,
    pub(crate) total_requests: AtomicU64,
}

impl McpRuntimeStats {
    pub(crate) fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.total_connections.load(Ordering::Relaxed),
            self.active_clients.load(Ordering::Relaxed) as u64,
            self.total_requests.load(Ordering::Relaxed),
        )
    }
}

pub(crate) fn publish_mcp_runtime_stats(
    data_source: &Arc<dyn AetowerMcpDataSource>,
    stats: &McpRuntimeStats,
) {
    let (total_connections, active_client_count, total_requests) = stats.snapshot();
    data_source.record_mcp_runtime_observation(
        total_connections,
        active_client_count,
        total_requests,
    );
}

pub fn is_socket_listener_reachable(socket_path: impl AsRef<Path>) -> bool {
    UnixStream::connect(socket_path.as_ref()).is_ok()
}

pub fn default_socket_path() -> PathBuf {
    if let Some(override_path) = env::var_os("AETOWER_MCP_SOCKET_PATH") {
        return PathBuf::from(override_path);
    }
    let base = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    base.join(".aetower").join("mcp.sock")
}

pub fn start_local_socket_server(
    data_source: Arc<dyn AetowerMcpDataSource>,
    socket_path: impl AsRef<Path>,
) -> Result<LocalMcpServerHandle, String> {
    let socket_path = socket_path.as_ref().to_path_buf();
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("create MCP socket directory {}: {error}", parent.display())
        })?;
        fs::set_permissions(parent, fs::Permissions::from_mode(SOCKET_DIR_MODE)).map_err(
            |error| {
                format!(
                    "set MCP socket directory permissions {}: {error}",
                    parent.display()
                )
            },
        )?;
    }
    if let Ok(metadata) = fs::symlink_metadata(&socket_path) {
        if !metadata.file_type().is_socket() {
            return Err(format!(
                "refusing to replace non-socket MCP path {}",
                socket_path.display()
            ));
        }
        fs::remove_file(&socket_path).map_err(|error| {
            format!("remove stale MCP socket {}: {error}", socket_path.display())
        })?;
    }

    let listener = UnixListener::bind(&socket_path)
        .map_err(|error| format!("bind MCP socket {}: {error}", socket_path.display()))?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(SOCKET_FILE_MODE)).map_err(
        |error| {
            format!(
                "set MCP socket permissions {}: {error}",
                socket_path.display()
            )
        },
    )?;
    let running = Arc::new(AtomicBool::new(true));
    let stats = Arc::new(McpRuntimeStats::default());
    let thread_running = Arc::clone(&running);
    let thread_socket_path = socket_path.clone();
    let client_threads: Arc<Mutex<Vec<thread::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    let thread_client_threads = Arc::clone(&client_threads);
    let thread_stats = Arc::clone(&stats);
    let join_handle = thread::spawn(move || {
        while thread_running.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if !thread_running.load(Ordering::SeqCst) {
                        break;
                    }
                    let source = Arc::clone(&data_source);
                    let connection_running = Arc::clone(&thread_running);
                    thread_stats
                        .total_connections
                        .fetch_add(1, Ordering::Relaxed);
                    thread_stats.active_clients.fetch_add(1, Ordering::Relaxed);
                    publish_mcp_runtime_stats(&source, &thread_stats);
                    let stats = Arc::clone(&thread_stats);
                    let dynamic_mode = DynamicExecutionMode::Local;
                    let join_handle = thread::spawn(move || {
                        if let Err(err) = handle_connection(
                            stream,
                            Arc::clone(&source),
                            dynamic_mode,
                            connection_running,
                            Arc::clone(&stats),
                        ) {
                            // Surface per-connection failures — handle_connection
                            // returns Ok on clean peer close, so reaching here
                            // means a real transport/framing fault the operator
                            // should see instead of the server reporting healthy
                            // while silently dropping every request.
                            eprintln!("aetower-mcp socket: handle_connection: {err}");
                        }
                        stats.active_clients.fetch_sub(1, Ordering::Relaxed);
                        publish_mcp_runtime_stats(&source, &stats);
                    });
                    if let Ok(mut handles) = thread_client_threads.lock() {
                        reap_finished_client_threads(&mut handles);
                        handles.push(join_handle);
                    }
                }
                Err(_) => break,
            }
        }
        if let Ok(mut handles) = thread_client_threads.lock() {
            reap_finished_client_threads(&mut handles);
        }
        let _ = fs::remove_file(thread_socket_path);
    });

    Ok(LocalMcpServerHandle {
        running,
        join_handle: Some(join_handle),
        client_threads,
        socket_path,
        stats,
    })
}

fn reap_finished_client_threads(handles: &mut Vec<thread::JoinHandle<()>>) {
    let mut remaining = Vec::with_capacity(handles.len());
    for handle in handles.drain(..) {
        if handle.is_finished() {
            let _ = handle.join();
        } else {
            remaining.push(handle);
        }
    }
    *handles = remaining;
}

pub fn proxy_stdio_to_socket(socket_path: impl AsRef<Path>) -> Result<(), String> {
    proxy_stdio_to_socket_polling(socket_path)
}

#[cfg(test)]
pub(crate) fn proxy_streams_to_socket<R, W>(
    mut input: R,
    output: W,
    socket_path: impl AsRef<Path>,
) -> Result<(), String>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let socket_path = socket_path.as_ref();
    let stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("connect MCP socket {}: {error}", socket_path.display()))?;
    let mut reader_stream = stream
        .try_clone()
        .map_err(|error| format!("clone socket stream: {error}"))?;
    let mut writer_stream = stream;

    let stdin_thread = thread::spawn(move || -> Result<(), String> {
        match std::io::copy(&mut input, &mut writer_stream) {
            Ok(_) => {}
            Err(ref err) if is_graceful_peer_close(err) => {}
            Err(err) => return Err(format!("copy stdin to MCP socket: {err}")),
        }
        // Signal to the peer that the client is done sending, but keep the
        // read half open so the stdout thread can drain any in-flight response.
        match writer_stream.shutdown(Shutdown::Write) {
            Ok(()) => Ok(()),
            Err(ref err) if is_graceful_peer_close(err) => Ok(()),
            Err(err) => Err(format!("shutdown MCP socket write-half: {err}")),
        }
    });

    let stdout_thread = thread::spawn(move || -> Result<(), String> {
        let mut output = output;
        let mut buffer = [0u8; 8192];
        loop {
            let bytes_read = match reader_stream.read(&mut buffer) {
                Ok(count) => count,
                Err(ref err) if is_graceful_peer_close(err) => 0,
                Err(err) => return Err(format!("read MCP socket: {err}")),
            };
            if bytes_read == 0 {
                break;
            }
            if let Err(err) = output.write_all(&buffer[..bytes_read]) {
                if is_graceful_peer_close(&err) {
                    break;
                }
                return Err(format!("write MCP socket to stdout: {err}"));
            }
            if let Err(err) = output.flush() {
                if is_graceful_peer_close(&err) {
                    break;
                }
                return Err(format!("flush stdout: {err}"));
            }
        }
        Ok(())
    });

    stdin_thread
        .join()
        .map_err(|_| "stdin proxy thread panicked".to_string())??;
    stdout_thread
        .join()
        .map_err(|_| "stdout proxy thread panicked".to_string())??;
    Ok(())
}

fn proxy_stdio_to_socket_polling(socket_path: impl AsRef<Path>) -> Result<(), String> {
    let socket_path = socket_path.as_ref();
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("connect MCP socket {}: {error}", socket_path.display()))?;
    stream
        .set_nonblocking(true)
        .map_err(|error| format!("set MCP socket nonblocking: {error}"))?;

    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();

    let stdin_fd = input.as_raw_fd();
    let socket_fd = stream.as_raw_fd();
    let original_parent_pid = current_parent_pid();
    let mut stdin_open = true;
    let mut write_half_open = true;
    let mut stdin_buffer = [0u8; 8192];
    let mut socket_buffer = [0u8; 8192];

    loop {
        if parent_exited(original_parent_pid) {
            break;
        }

        let mut fds = [
            libc::pollfd {
                fd: stdin_fd,
                events: if stdin_open { libc::POLLIN } else { 0 },
                revents: 0,
            },
            libc::pollfd {
                fd: socket_fd,
                events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
                revents: 0,
            },
        ];

        let poll_result = unsafe {
            libc::poll(
                fds.as_mut_ptr(),
                fds.len() as libc::nfds_t,
                PROXY_POLL_TIMEOUT.as_millis() as i32,
            )
        };
        if poll_result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("poll MCP proxy fds: {error}"));
        }
        if poll_result == 0 {
            continue;
        }

        if stdin_open && (fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0) {
            match input.read(&mut stdin_buffer) {
                Ok(0) => {
                    stdin_open = false;
                    if write_half_open {
                        match stream.shutdown(Shutdown::Write) {
                            Ok(()) => {
                                write_half_open = false;
                            }
                            Err(ref err) if is_graceful_peer_close(err) => {
                                write_half_open = false;
                            }
                            Err(err) => {
                                return Err(format!("shutdown MCP socket write-half: {err}"));
                            }
                        }
                    }
                }
                Ok(bytes_read) => {
                    let mut written = 0;
                    while written < bytes_read {
                        match stream.write(&stdin_buffer[written..bytes_read]) {
                            Ok(0) => break,
                            Ok(count) => written += count,
                            Err(ref err)
                                if err.kind() == std::io::ErrorKind::WouldBlock
                                    || err.kind() == std::io::ErrorKind::Interrupted =>
                            {
                                thread::sleep(Duration::from_millis(10));
                            }
                            Err(ref err) if is_graceful_peer_close(err) => return Ok(()),
                            Err(err) => {
                                return Err(format!("write stdin payload to MCP socket: {err}"));
                            }
                        }
                    }
                }
                Err(ref err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(err) => return Err(format!("read stdin for MCP proxy: {err}")),
            }
        }

        if fds[1].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
            loop {
                match stream.read(&mut socket_buffer) {
                    Ok(0) => return Ok(()),
                    Ok(bytes_read) => {
                        output
                            .write_all(&socket_buffer[..bytes_read])
                            .map_err(|err| format!("write MCP socket to stdout: {err}"))?;
                        output
                            .flush()
                            .map_err(|err| format!("flush stdout: {err}"))?;
                    }
                    Err(ref err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::Interrupted =>
                    {
                        break;
                    }
                    Err(ref err) if is_graceful_peer_close(err) => return Ok(()),
                    Err(err) => return Err(format!("read MCP socket: {err}")),
                }
            }
        }
    }

    Ok(())
}

fn current_parent_pid() -> libc::pid_t {
    unsafe { libc::getppid() }
}

fn parent_exited(original_parent_pid: libc::pid_t) -> bool {
    let current = current_parent_pid();
    current <= 1 || current != original_parent_pid
}

pub(crate) fn is_graceful_peer_close(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::UnexpectedEof
    )
}

pub(crate) struct McpSocketConnection {
    reader: BufReader<UnixStream>,
}

impl McpSocketConnection {
    pub(crate) fn new(stream: UnixStream) -> Result<Self, String> {
        stream
            .set_read_timeout(Some(MCP_READ_TIMEOUT))
            .map_err(|error| format!("set MCP socket read timeout: {error}"))?;
        Ok(Self {
            reader: BufReader::new(stream),
        })
    }

    pub(crate) fn read_message(
        &mut self,
        framing: &mut Option<MessageFraming>,
    ) -> Result<ReadMessageOutcome, String> {
        read_message(&mut self.reader, framing)
    }

    pub(crate) fn write_message(
        &mut self,
        message: &Value,
        framing: MessageFraming,
    ) -> Result<(), String> {
        write_message(self.reader.get_mut(), message, framing)
    }

    pub(crate) fn shutdown(&mut self) {
        let _ = self.reader.get_mut().shutdown(Shutdown::Both);
    }
}

impl Drop for McpSocketConnection {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn handle_connection(
    stream: UnixStream,
    data_source: Arc<dyn AetowerMcpDataSource>,
    dynamic_mode: DynamicExecutionMode,
    running: Arc<AtomicBool>,
    stats: Arc<McpRuntimeStats>,
) -> Result<(), String> {
    let mut connection = McpSocketConnection::new(stream)?;
    let server = AetowerMcpServer::new_with_stats(data_source, dynamic_mode, Some(stats));
    let mut framing = None;

    loop {
        match connection.read_message(&mut framing)? {
            ReadMessageOutcome::Message(message) => {
                let response = server.handle_message(message);
                if let Some(response) = response {
                    connection.write_message(
                        &response,
                        framing.unwrap_or(MessageFraming::ContentLength),
                    )?;
                }
            }
            ReadMessageOutcome::EndOfStream => break,
            ReadMessageOutcome::Timeout if running.load(Ordering::SeqCst) => continue,
            ReadMessageOutcome::Timeout => break,
        }
    }
    connection.shutdown();
    Ok(())
}

pub(crate) fn jsonrpc_error(code: i64, message: impl Into<String>) -> Value {
    json!({
        "code": code,
        "message": message.into(),
    })
}

pub(crate) fn read_message(
    reader: &mut impl Read,
    framing: &mut Option<MessageFraming>,
) -> Result<ReadMessageOutcome, String> {
    if matches!(framing, Some(MessageFraming::JsonLine)) {
        return read_json_line_message(reader);
    }

    let mut headers = Vec::new();
    let mut byte = [0u8; 1];
    let mut recent = Vec::new();

    loop {
        match reader.read(&mut byte) {
            Ok(0) => {
                if headers.is_empty() {
                    return Ok(ReadMessageOutcome::EndOfStream);
                }
                let trimmed = trim_ascii_whitespace(&headers);
                if is_json_line_candidate(trimmed) {
                    *framing = Some(MessageFraming::JsonLine);
                    return parse_json_line(trimmed);
                }
                return Err("unexpected EOF while reading headers".into());
            }
            Ok(_) => {
                headers.push(byte[0]);
                let trimmed = trim_ascii_whitespace(&headers);
                if is_json_line_candidate(trimmed) && byte[0] == b'\n' {
                    *framing = Some(MessageFraming::JsonLine);
                    return parse_json_line(trimmed);
                }
                recent.push(byte[0]);
                if recent.len() > 4 {
                    recent.remove(0);
                }
                if recent.ends_with(b"\r\n\r\n") || recent.ends_with(b"\n\n") {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(ReadMessageOutcome::Timeout);
            }
            Err(error) => return Err(format!("read header: {error}")),
        }
    }

    let header_text =
        String::from_utf8(headers).map_err(|error| format!("header utf8: {error}"))?;
    let content_length = header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .ok_or_else(|| "missing Content-Length".to_string())?;

    let mut body = vec![0u8; content_length];
    let mut offset = 0;
    while offset < content_length {
        match reader.read(&mut body[offset..]) {
            Ok(0) => return Err("unexpected EOF while reading body".into()),
            Ok(read) => offset += read,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(format!("read body: {error}")),
        }
    }
    let value = serde_json::from_slice(&body).map_err(|error| format!("parse json: {error}"))?;
    *framing = Some(MessageFraming::ContentLength);
    Ok(ReadMessageOutcome::Message(value))
}

fn read_json_line_message(reader: &mut impl Read) -> Result<ReadMessageOutcome, String> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        match reader.read(&mut byte) {
            Ok(0) => {
                if buffer.is_empty() {
                    return Ok(ReadMessageOutcome::EndOfStream);
                }
                return parse_json_line(trim_ascii_whitespace(&buffer));
            }
            Ok(_) => {
                buffer.push(byte[0]);
                if byte[0] == b'\n' {
                    return parse_json_line(trim_ascii_whitespace(&buffer));
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(ReadMessageOutcome::Timeout);
            }
            Err(error) => return Err(format!("read json line: {error}")),
        }
    }
}

fn is_json_line_candidate(bytes: &[u8]) -> bool {
    matches!(bytes.first(), Some(b'{') | Some(b'['))
}

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn parse_json_line(bytes: &[u8]) -> Result<ReadMessageOutcome, String> {
    if bytes.is_empty() {
        return Ok(ReadMessageOutcome::Timeout);
    }
    let value =
        serde_json::from_slice(bytes).map_err(|error| format!("parse json line: {error}"))?;
    Ok(ReadMessageOutcome::Message(value))
}

pub(crate) fn write_message(
    writer: &mut impl Write,
    message: &Value,
    framing: MessageFraming,
) -> Result<(), String> {
    let body =
        serde_json::to_vec(message).map_err(|error| format!("serialize response: {error}"))?;
    match framing {
        MessageFraming::ContentLength => {
            write!(writer, "Content-Length: {}\r\n\r\n", body.len())
                .map_err(|error| format!("write header: {error}"))?;
            writer
                .write_all(&body)
                .map_err(|error| format!("write body: {error}"))?;
        }
        MessageFraming::JsonLine => {
            writer
                .write_all(&body)
                .map_err(|error| format!("write json line: {error}"))?;
            writer
                .write_all(b"\n")
                .map_err(|error| format!("write json line terminator: {error}"))?;
        }
    }
    writer.flush().map_err(|error| format!("flush: {error}"))
}

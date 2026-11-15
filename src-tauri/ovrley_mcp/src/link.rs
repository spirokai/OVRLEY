//! The local connection between the `ovrley --mcp` process and the running window (D13).
//!
//! - **Transport:** a Unix socket on macOS and Linux, a named pipe on Windows, through
//!   `interprocess`'s synchronous local sockets. Only the same user can connect: the socket file is
//!   `0600` in a `0700` directory, and the pipe's name carries the user name and rejects remote
//!   clients.
//! - **Who serves:** the first window binds the socket. A later instance finds it answering, logs
//!   that it is not serving, and its indicator never shows (Q6). A socket file left by a crash does
//!   not answer and is replaced.
//! - **Wire format:** one JSON object per line. A request carries an id, the tool name and its
//!   arguments; the reply carries the id and the outcome.
//! - **Sessions:** each connection is one agent session, for the header's indicator. A call is
//!   handed to the window's UI thread through the hooks; the connection's thread waits for the
//!   reply, up to [`CALL_TIMEOUT`].

use crate::catalog::ToolOutcome;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ListenerOptions, Name, Stream};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long a call waits for the window to answer.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// What an agent sees when no window serves.
pub const NOT_OPEN: &str = "OVRLEY is not open. Start OVRLEY, then try again.";

/// Where the window listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocketLocation {
    /// A socket file (macOS, Linux).
    File(PathBuf),
    /// A named pipe in the local namespace (Windows).
    Namespaced(String),
}

impl SocketLocation {
    /// The location for this user. On Linux the session's runtime directory is preferred, which is
    /// already private to the user; elsewhere the app-data directory.
    pub fn for_user(app_data_dir: &Path) -> Self {
        if cfg!(windows) {
            let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
            return Self::Namespaced(format!("ovrley-mcp-{user}"));
        }
        let runtime_dir = if cfg!(target_os = "linux") {
            std::env::var_os("XDG_RUNTIME_DIR")
                .map(|dir| PathBuf::from(dir).join("com.ovrley.desktop"))
        } else {
            None
        };
        Self::File(
            runtime_dir
                .unwrap_or_else(|| app_data_dir.join("mcp"))
                .join("ovrley.sock"),
        )
    }

    fn name(&self) -> std::io::Result<Name<'_>> {
        match self {
            Self::File(path) => path
                .as_path()
                .to_fs_name::<interprocess::local_socket::GenericFilePath>(),
            Self::Namespaced(name) => name
                .as_str()
                .to_ns_name::<interprocess::local_socket::GenericNamespaced>(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkRequest {
    pub id: u64,
    pub tool: String,
    pub arguments: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkReply {
    pub id: u64,
    pub outcome: ToolOutcome,
}

/// The `--mcp` process's side: one connection for the process's lifetime, which is one agent
/// session. A broken connection is opened again once per call.
pub struct LinkClient {
    location: SocketLocation,
    connection: Option<BufReader<Stream>>,
    next_id: u64,
}

impl LinkClient {
    /// Connects at once when a window serves, so the indicator shows as soon as the agent starts.
    pub fn new(location: SocketLocation) -> Self {
        let mut client = Self {
            location,
            connection: None,
            next_id: 1,
        };
        client.connection = client.connect().ok();
        client
    }

    fn connect(&self) -> std::io::Result<BufReader<Stream>> {
        Ok(BufReader::new(Stream::connect(self.location.name()?)?))
    }

    fn exchange(
        connection: &mut BufReader<Stream>,
        request: &LinkRequest,
    ) -> std::io::Result<LinkReply> {
        let mut line = serde_json::to_string(request).map_err(std::io::Error::other)?;
        line.push('\n');
        connection.get_mut().write_all(line.as_bytes())?;
        connection.get_mut().flush()?;
        let mut reply = String::new();
        if connection.read_line(&mut reply)? == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        serde_json::from_str(&reply).map_err(std::io::Error::other)
    }

    /// Runs a tool in the window. With no window, the call fails with [`NOT_OPEN`].
    pub fn call(&mut self, tool: &str, arguments: Value) -> ToolOutcome {
        let request = LinkRequest {
            id: self.next_id,
            tool: tool.to_string(),
            arguments,
        };
        self.next_id += 1;
        for attempt in 0..2 {
            if self.connection.is_none() {
                match self.connect() {
                    Ok(connection) => self.connection = Some(connection),
                    Err(_) => return ToolOutcome::Failed(NOT_OPEN.into()),
                }
            }
            let connection = self.connection.as_mut().expect("connected above");
            match Self::exchange(connection, &request) {
                Ok(reply) if reply.id == request.id => return reply.outcome,
                Ok(reply) => {
                    log::error!(
                        "the window answered call {} with id {}",
                        request.id,
                        reply.id
                    );
                    self.connection = None;
                }
                Err(error) => {
                    log::debug!("the connection to the window broke (attempt {attempt}): {error}");
                    self.connection = None;
                }
            }
        }
        ToolOutcome::Failed(NOT_OPEN.into())
    }
}

/// How the window's bridge reaches the app. Both run on connection threads; the implementations
/// hand the work to the UI thread.
pub struct BridgeHooks {
    /// Runs a tool on the UI thread and returns its outcome (waiting up to [`CALL_TIMEOUT`]).
    pub call: Arc<dyn Fn(String, Value) -> ToolOutcome + Send + Sync>,
    /// An agent session opened (`true`) or closed (`false`).
    pub session: Arc<dyn Fn(bool) + Send + Sync>,
}

/// The window's listener. Dropping it removes the socket file; its threads end with the process.
pub struct LinkServer {
    location: SocketLocation,
}

impl Drop for LinkServer {
    fn drop(&mut self) {
        if let SocketLocation::File(path) = &self.location {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn prepare_directory(location: &SocketLocation) -> std::io::Result<()> {
    let SocketLocation::File(path) = location else {
        return Ok(());
    };
    let directory = path.parent().expect("a socket file lives in a directory");
    std::fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Starts serving tool calls, unless another OVRLEY window already does (`Ok(None)`).
pub fn serve(location: SocketLocation, hooks: BridgeHooks) -> std::io::Result<Option<LinkServer>> {
    if Stream::connect(location.name()?).is_ok() {
        log::info!("another OVRLEY window serves agents; this one does not");
        return Ok(None);
    }
    prepare_directory(&location)?;
    // Nothing answered, so a socket file still there was left by a crash: replace it.
    let options = ListenerOptions::new()
        .name(location.name()?)
        .try_overwrite(true);
    // Linux sets the socket's mode before binding. macOS cannot; there the `0700` directory keeps
    // other users out, and the file is narrowed after binding.
    #[cfg(target_os = "linux")]
    let options = {
        use interprocess::os::unix::local_socket::ListenerOptionsExt;
        options.mode(0o600)
    };
    let listener = options.create_sync()?;
    #[cfg(all(unix, not(target_os = "linux")))]
    if let SocketLocation::File(path) = &location {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    let hooks = Arc::new(hooks);
    std::thread::Builder::new()
        .name("mcp-listener".into())
        .spawn(move || {
            for connection in listener.incoming() {
                match connection {
                    Ok(stream) => {
                        let hooks = hooks.clone();
                        let spawned = std::thread::Builder::new()
                            .name("mcp-session".into())
                            .spawn(move || run_session(stream, &hooks));
                        if let Err(error) = spawned {
                            log::warn!("could not start an agent session: {error}");
                        }
                    }
                    Err(error) => log::warn!("an agent connection failed: {error}"),
                }
            }
        })?;
    log::info!("serving agents at {location:?}");
    Ok(Some(LinkServer { location }))
}

fn run_session(stream: Stream, hooks: &BridgeHooks) {
    (hooks.session)(true);
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                log::debug!("an agent session ended: {error}");
                break;
            }
        }
        let reply = match serde_json::from_str::<LinkRequest>(&line) {
            Ok(request) => LinkReply {
                id: request.id,
                outcome: (hooks.call)(request.tool, request.arguments),
            },
            Err(error) => {
                log::warn!("an agent sent a malformed request: {error}");
                break;
            }
        };
        let mut text = serde_json::to_string(&reply).expect("a reply serializes");
        text.push('\n');
        if reader
            .get_mut()
            .write_all(text.as_bytes())
            .and_then(|()| reader.get_mut().flush())
            .is_err()
        {
            break;
        }
    }
    (hooks.session)(false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicIsize, Ordering};

    fn temp_location(label: &str) -> SocketLocation {
        if cfg!(windows) {
            return SocketLocation::Namespaced(format!(
                "ovrley-mcp-test-{label}-{}",
                std::process::id()
            ));
        }
        // Short, because socket paths are limited to about 100 bytes.
        let dir = std::env::temp_dir().join(format!("om-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        SocketLocation::File(dir.join("s.sock"))
    }

    fn echo_hooks(sessions: Arc<AtomicIsize>) -> BridgeHooks {
        let opened = sessions.clone();
        BridgeHooks {
            call: Arc::new(|tool, arguments| {
                ToolOutcome::Ok(json!({ "tool": tool, "arguments": arguments }))
            }),
            session: Arc::new(move |open| {
                opened.fetch_add(if open { 1 } else { -1 }, Ordering::SeqCst);
            }),
        }
    }

    fn wait_for(condition: impl Fn() -> bool) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out");
    }

    #[test]
    fn a_call_round_trips_and_a_connection_is_a_session() {
        let location = temp_location("roundtrip");
        let sessions = Arc::new(AtomicIsize::new(0));
        let server = serve(location.clone(), echo_hooks(sessions.clone()))
            .unwrap()
            .expect("serving");
        let mut client = LinkClient::new(location.clone());
        wait_for(|| sessions.load(Ordering::SeqCst) == 1);
        assert_eq!(
            client.call("drawer_dismiss", json!({})),
            ToolOutcome::Ok(json!({"tool": "drawer_dismiss", "arguments": {}}))
        );
        assert_eq!(
            client.call("ovrley_summary", Value::Null),
            ToolOutcome::Ok(json!({"tool": "ovrley_summary", "arguments": null}))
        );
        drop(client);
        wait_for(|| sessions.load(Ordering::SeqCst) == 0);

        #[cfg(unix)]
        if let SocketLocation::File(path) = &location {
            use std::os::unix::fs::PermissionsExt;
            let socket = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            let directory = std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!((socket, directory), (0o600, 0o700));
        }
        drop(server);
    }

    #[test]
    fn a_second_window_does_not_serve() {
        let location = temp_location("second");
        let _first = serve(location.clone(), echo_hooks(Arc::new(AtomicIsize::new(0))))
            .unwrap()
            .expect("serving");
        assert!(serve(location, echo_hooks(Arc::new(AtomicIsize::new(0))))
            .unwrap()
            .is_none());
    }

    #[test]
    fn with_no_window_a_call_says_ovrley_is_not_open() {
        let mut client = LinkClient::new(temp_location("none"));
        assert_eq!(
            client.call("ovrley_summary", Value::Null),
            ToolOutcome::Failed(NOT_OPEN.into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_socket_file_is_replaced() {
        let location = temp_location("stale");
        let SocketLocation::File(path) = &location else {
            unreachable!()
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A socket nobody listens on any more, as a crash leaves it.
        drop(std::os::unix::net::UnixListener::bind(path).unwrap());
        assert!(path.exists());
        let server = serve(location.clone(), echo_hooks(Arc::new(AtomicIsize::new(0)))).unwrap();
        assert!(server.is_some());
        let mut client = LinkClient::new(location);
        assert!(matches!(
            client.call("drawer_dismiss", json!({})),
            ToolOutcome::Ok(_)
        ));
    }
}

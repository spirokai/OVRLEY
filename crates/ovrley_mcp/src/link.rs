//! The connection between the `ovrley --mcp` process and the running window.
//!
//! - **Transport:** a Unix socket on macOS and Linux, a named pipe on Windows. The socket file is
//!   `0600` in a `0700` directory. The pipe's name carries the user's name, it rejects remote
//!   clients and it has Windows' default access rules.
//! - **Who serves:** the first window, which holds a lock beside the socket until it closes. A
//!   socket file left by a crash is replaced.
//! - **Wire format:** one JSON object per line. A request carries an id, the tool's name and its
//!   arguments; the reply carries the id and the result.
//! - **Sessions:** each connection is one agent session, for the header's indicator.
//! - **Waiting:** a call has 10 seconds for its dispatch. One that then answers later (a
//!   worker's result or the user's answer) has no limit.
//! - **Delivery:** at most once. A call is sent again only when it could not be written, because
//!   one that was delivered may have made its change.

use crate::catalog::{self, Answer, ToolError, ToolResult};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Name, Stream,
};
use ovrley_app::store::agent::{AgentReply, CANCELLED};
use ovrley_app::store::DispatchHandle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{File, TryLockError};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

/// How long a call waits for its dispatch. A longer one means the UI thread is stuck.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// The pause after a failed accept, so a persistent error (no free file descriptors) cannot spin.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);

/// What an agent reads when no window serves.
pub(crate) const NOT_OPEN: &str = "OVRLEY is not open. Start OVRLEY, then try again.";
const CONNECTION_LOST: &str =
    "The connection to OVRLEY was lost before it answered. The change may still be applied.";
const BUSY: &str = "OVRLEY is busy. Try again.";
const CLOSING: &str = "OVRLEY is closing.";
const NO_ANSWER: &str = "OVRLEY did not answer in time. The change may still be applied.";

/// Where the window listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocketLocation {
    /// A socket file (macOS, Linux).
    File(PathBuf),
    /// A named pipe in the local namespace (Windows).
    Namespaced(String),
}

impl SocketLocation {
    /// The location for this user.
    ///
    /// Linux prefers the session's runtime directory, which is private to the user, when it is
    /// set to an absolute path. Elsewhere it is the app-data directory.
    pub fn for_user(app_data_dir: &Path) -> Self {
        if cfg!(windows) {
            let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
            return Self::Namespaced(format!("ovrley-mcp-{user}"));
        }
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|_| cfg!(target_os = "linux"))
            .map(PathBuf::from)
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(ovrley_services::runtime::APP_IDENTIFIER));
        let directory = runtime_dir.unwrap_or_else(|| app_data_dir.join("mcp"));
        Self::File(directory.join("ovrley.sock"))
    }

    fn name(&self) -> io::Result<Name<'_>> {
        match self {
            Self::File(path) => path.as_path().to_fs_name::<GenericFilePath>(),
            Self::Namespaced(name) => name.as_str().to_ns_name::<GenericNamespaced>(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    id: u64,
    tool: String,
    arguments: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    id: u64,
    result: ToolResult,
}

fn write_line(output: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    output.write_all(&line)
}

/// The `--mcp` process's end: one connection, which is one agent session.
#[derive(Debug)]
pub struct LinkClient {
    location: SocketLocation,
    connection: Option<BufReader<Stream>>,
    next_id: u64,
}

impl LinkClient {
    /// Connects at once when a window serves, so the indicator shows as soon as the agent starts.
    #[must_use]
    pub fn new(location: SocketLocation) -> Self {
        let mut client = Self {
            location,
            connection: None,
            next_id: 1,
        };
        client.connection = client.connect().ok();
        client
    }

    fn connect(&self) -> io::Result<BufReader<Stream>> {
        Ok(BufReader::new(Stream::connect(self.location.name()?)?))
    }

    /// Writes the request, on a new connection when the old one no longer takes it.
    fn send(&mut self, request: &Request) -> io::Result<BufReader<Stream>> {
        if let Some(mut connection) = self.connection.take() {
            match write_line(connection.get_mut(), request) {
                Ok(()) => return Ok(connection),
                Err(error) => log::debug!("connecting to the window again: {error}"),
            }
        }
        let mut connection = self.connect()?;
        write_line(connection.get_mut(), request)?;
        Ok(connection)
    }

    fn receive(connection: &mut BufReader<Stream>) -> io::Result<Response> {
        connection.get_mut().flush()?;
        let mut line = String::new();
        if connection.read_line(&mut line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        serde_json::from_str(&line).map_err(io::Error::from)
    }

    /// Runs a tool in the window, and waits for its result.
    ///
    /// # Errors
    ///
    /// The tool's own error, or a failed call when no window serves or the connection is lost.
    pub fn call(&mut self, tool: &str, arguments: Value) -> ToolResult {
        let request = Request {
            id: self.next_id,
            tool: tool.to_string(),
            arguments,
        };
        self.next_id += 1;
        let mut connection = self.send(&request).map_err(|error| {
            log::debug!("no window takes calls: {error}");
            ToolError::Failed(NOT_OPEN.into())
        })?;
        match Self::receive(&mut connection) {
            Ok(response) if response.id == request.id => {
                self.connection = Some(connection);
                return response.result;
            }
            Ok(response) => log::error!(
                "the window answered call {} with id {}",
                request.id,
                response.id
            ),
            Err(error) => log::debug!("the window did not answer call {}: {error}", request.id),
        }
        Err(ToolError::Failed(CONNECTION_LOST.into()))
    }
}

/// Work for the window's UI thread.
pub type UiJob = Box<dyn FnOnce(&DispatchHandle) + Send>;

/// The window's event loop has ended.
#[derive(Clone, Copy, Debug)]
pub struct WindowClosed;

impl std::fmt::Display for WindowClosed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the window's event loop has ended")
    }
}

impl std::error::Error for WindowClosed {}

/// The window, as the socket's threads reach it.
pub trait Window: Send + Sync + 'static {
    /// Queues `job` on the UI thread, outside any dispatch.
    ///
    /// # Errors
    ///
    /// [`WindowClosed`] when the event loop no longer takes work.
    fn post(&self, job: UiJob) -> Result<(), WindowClosed>;
}

/// Runs one call as a dispatch on the UI thread and waits for its result.
fn call_window(window: &impl Window, tool: String, arguments: Value) -> ToolResult {
    let (answered, answer) = mpsc::channel();
    // Apart from the answer: a call that fails at once drops its reply, which sends "cancelled"
    // before the dispatch has returned the reason.
    let (replied, reply) = mpsc::channel();
    let agent_reply = AgentReply::new(move |result| {
        // A call answered in its dispatch has nobody reading this.
        let _ = replied.send(result);
    });
    let job: UiJob = Box::new(move |dispatch| {
        let outcome = dispatch
            .dispatch_with_result(move |store, services| {
                catalog::call(&tool, arguments, store, services, agent_reply)
            })
            .unwrap_or_else(|| Answer::Now(Err(ToolError::Failed(BUSY.into()))));
        // The connection stops listening at the timeout.
        let _ = answered.send(outcome);
    });
    window
        .post(job)
        .map_err(|WindowClosed| ToolError::Failed(CLOSING.into()))?;
    match answer.recv_timeout(CALL_TIMEOUT) {
        Ok(Answer::Now(result)) => result,
        Ok(Answer::Later) => reply
            .recv()
            .unwrap_or_else(|_| Err(CANCELLED.into()))
            .map_err(ToolError::Failed),
        Err(_) => Err(ToolError::Failed(NO_ANSWER.into())),
    }
}

/// The window's listener. Dropping it removes the socket file; its threads end with the process.
#[derive(Debug)]
pub struct LinkServer {
    location: SocketLocation,
    /// Released after the socket file is removed, so only the serving window ever removes it.
    _lock: Option<File>,
}

impl Drop for LinkServer {
    fn drop(&mut self) {
        if let SocketLocation::File(path) = &self.location {
            if let Err(error) = std::fs::remove_file(path) {
                log::debug!("could not remove {}: {error}", path.display());
            }
        }
    }
}

/// Makes the socket's private directory and returns it.
fn prepare_directory(socket: &Path) -> io::Result<&Path> {
    let directory = socket.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no directory", socket.display()),
        )
    })?;
    std::fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory)
}

/// Starts serving tool calls to `window`, unless another window serves already (`Ok(None)`).
///
/// # Errors
///
/// When the socket, its directory, its lock or the listening thread cannot be made.
pub fn serve(location: SocketLocation, window: impl Window) -> io::Result<Option<LinkServer>> {
    let lock = match &location {
        SocketLocation::File(socket) => {
            let path = prepare_directory(socket)?.join("ovrley.lock");
            let file = File::options()
                .create(true)
                .write(true)
                .truncate(false)
                .open(path)?;
            match file.try_lock() {
                Ok(()) => Some(file),
                Err(TryLockError::WouldBlock) => {
                    log::info!("another OVRLEY window serves agents; this one does not");
                    return Ok(None);
                }
                Err(TryLockError::Error(error)) => return Err(error),
            }
        }
        // Only one process can create the first instance of a named pipe.
        SocketLocation::Namespaced(_) => None,
    };
    if Stream::connect(location.name()?).is_ok() {
        log::info!("the socket already answers; this window does not serve agents");
        return Ok(None);
    }
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
    let window = Arc::new(window);
    std::thread::Builder::new()
        .name("mcp-listener".into())
        .spawn(move || {
            for connection in listener.incoming() {
                match connection {
                    Ok(stream) => {
                        let window = Arc::clone(&window);
                        let spawned = std::thread::Builder::new()
                            .name("mcp-session".into())
                            .spawn(move || run_session(stream, &*window));
                        if let Err(error) = spawned {
                            log::warn!("could not start an agent session: {error}");
                        }
                    }
                    Err(error) => {
                        log::warn!("an agent connection failed: {error}");
                        std::thread::sleep(ACCEPT_RETRY_DELAY);
                    }
                }
            }
        })?;
    log::info!("serving agents at {location:?}");
    Ok(Some(LinkServer {
        location,
        _lock: lock,
    }))
}

fn run_session(stream: Stream, window: &impl Window) {
    let opened = window.post(Box::new(|dispatch| {
        dispatch.dispatch(|store, _| store.agent_session.session_opened());
    }));
    if let Err(error) = opened {
        log::debug!("no agent session opens: {error}");
        return;
    }
    let mut connection = BufReader::new(stream);
    if let Err(error) = answer_requests(&mut connection, window) {
        log::debug!("an agent session ended: {error}");
    }
    let closed = window.post(Box::new(|dispatch| {
        dispatch.dispatch(|store, _| store.agent_session.session_closed());
    }));
    if let Err(error) = closed {
        log::debug!("an agent session outlived the window: {error}");
    }
}

fn answer_requests(connection: &mut BufReader<Stream>, window: &impl Window) -> io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        connection.read_line(&mut line)?;
        // A line without its end was cut off while it was written, and the client sends it again.
        if !line.ends_with('\n') {
            return Ok(());
        }
        let request: Request = serde_json::from_str(&line)?;
        let response = Response {
            id: request.id,
            result: call_window(window, request.tool, request.arguments),
        };
        write_line(connection.get_mut(), &response)?;
        connection.get_mut().flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{socket, UiThread};
    use serde_json::json;
    use std::time::Instant;

    fn wait_for(condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_call_round_trips_and_a_connection_is_a_session() {
        let location = socket("roundtrip");
        let window = UiThread::start();
        let _server = serve(location.clone(), window.clone())
            .unwrap()
            .expect("serving");
        let mut client = LinkClient::new(location.clone());
        wait_for(|| window.connections() == 1);
        let drawer = client.call("drawer_dismiss", json!({})).unwrap();
        assert_eq!(drawer["drawer"]["visible"], false);

        let at_once = client.call("project_save", json!({}));
        assert!(
            matches!(&at_once, Err(ToolError::Failed(reason)) if reason != CANCELLED),
            "a call that fails at once keeps its reason: {at_once:?}"
        );
        let later = client.call("activity_import", json!({"path": "/nowhere/ride.fit"}));
        assert!(
            matches!(&later, Err(ToolError::Failed(reason)) if reason.contains("/nowhere/ride.fit")),
            "a call answered later gets the worker's reason: {later:?}"
        );
        drop(client);
        wait_for(|| window.connections() == 0);

        #[cfg(unix)]
        if let SocketLocation::File(path) = &location {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!((mode(path), mode(path.parent().unwrap())), (0o600, 0o700));
        }
    }

    #[test]
    fn only_one_of_the_windows_started_at_once_serves() {
        let location = socket("second");
        let start = Arc::new(std::sync::Barrier::new(4));
        let windows: Vec<_> = (0..4)
            .map(|_| {
                let (location, start) = (location.clone(), Arc::clone(&start));
                std::thread::spawn(move || {
                    let window = UiThread::start();
                    start.wait();
                    serve(location, window).unwrap()
                })
            })
            .collect();
        let servers: Vec<_> = windows
            .into_iter()
            .map(|window| window.join().unwrap())
            .collect();
        assert_eq!(servers.iter().flatten().count(), 1);
    }

    #[test]
    fn with_no_window_a_call_says_ovrley_is_not_open() {
        let mut client = LinkClient::new(socket("none"));
        assert_eq!(
            client.call("ovrley_summary", Value::Null),
            Err(ToolError::Failed(NOT_OPEN.into()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_socket_file_is_replaced() {
        let location = socket("stale");
        let SocketLocation::File(path) = &location else {
            unreachable!("Unix sockets are files")
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A socket nobody listens on any more, as a crash leaves it.
        drop(std::os::unix::net::UnixListener::bind(path).unwrap());
        assert!(path.exists());
        let _server = serve(location.clone(), UiThread::start())
            .unwrap()
            .expect("serving");
        let mut client = LinkClient::new(location);
        assert!(client.call("drawer_dismiss", json!({})).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_delivered_call_is_never_sent_again() {
        let location = socket("once");
        let SocketLocation::File(path) = &location else {
            unreachable!("Unix sockets are files")
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(path).unwrap();
        let (received, requests) = mpsc::channel();
        // Takes each call and drops the connection, as a window that goes away does.
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                BufReader::new(stream).read_line(&mut line).unwrap();
                if received.send(line).is_err() {
                    break;
                }
            }
        });
        let mut client = LinkClient::new(location);
        assert_eq!(
            client.call("widget_add", json!({})),
            Err(ToolError::Failed(CONNECTION_LOST.into()))
        );
        assert!(requests.recv().unwrap().contains("widget_add"));
        assert!(requests.recv_timeout(Duration::from_millis(200)).is_err());
    }
}

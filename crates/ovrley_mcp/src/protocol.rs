//! MCP over standard input and output, for the `ovrley --mcp` process.
//!
//! - **Framing:** newline-delimited JSON-RPC 2.0. Standard output carries only protocol messages;
//!   logs go to standard error.
//! - **Methods:** `initialize`, `ping`, `tools/list` and `tools/call`. Notifications need no
//!   answer. A call runs in the window and cannot be cancelled from here.
//! - **Order:** this process answers everything but `tools/call` itself, at once, also while a
//!   call waits for the user. Calls run one at a time, in the order they came.
//! - **Versions:** an `initialize` asking for a revision this server serves gets it back; any
//!   other gets the newest. Batches are accepted, as revision 2025-03-26 requires.
//! - **Errors:** a tool that ran and failed is a result with `isError`. Malformed arguments and
//!   unknown tools are invalid-params errors.

use crate::catalog::{self, ToolError, ToolResult};
use crate::link::LinkClient;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::sync::{mpsc, Mutex};

/// The revisions this server serves, newest first.
const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

const INSTRUCTIONS: &str = "OVRLEY turns activity data into video overlays. These tools drive \
    the OVRLEY window that is open on this computer, as the person at it would. Start with \
    ovrley_summary to see its state. The person must turn on \"Allow agent edits\" in OVRLEY's \
    Settings first.";

/// Runs tools somewhere: in the window through [`LinkClient`], or in a test.
pub trait ToolCaller {
    /// Runs one tool and waits for its result.
    ///
    /// # Errors
    ///
    /// The tool's [`ToolError`].
    fn call(&mut self, tool: &str, arguments: Value) -> ToolResult;
}

impl ToolCaller for LinkClient {
    fn call(&mut self, tool: &str, arguments: Value) -> ToolResult {
        LinkClient::call(self, tool, arguments)
    }
}

fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

struct ToolCall {
    id: Value,
    name: String,
    arguments: Value,
}

impl ToolCall {
    fn run(self, caller: &mut impl ToolCaller) -> Value {
        match caller.call(&self.name, self.arguments) {
            Ok(value) => {
                // `{:#}` is the indented form.
                let text = format!("{value:#}");
                let mut result = json!({ "content": [{ "type": "text", "text": text }] });
                if value.is_object() {
                    result["structuredContent"] = value;
                }
                result["isError"] = json!(false);
                result_response(self.id, result)
            }
            Err(ToolError::InvalidParams(message)) => {
                error_response(self.id, INVALID_PARAMS, message)
            }
            Err(ToolError::Failed(message)) => result_response(
                self.id,
                json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
            ),
        }
    }
}

/// What answers one request.
enum Response {
    Ready(Value),
    /// The window's result, once it has one.
    Call(ToolCall),
}

impl Response {
    fn resolve(self, caller: &mut impl ToolCaller) -> Value {
        match self {
            Self::Ready(value) => value,
            Self::Call(call) => call.run(caller),
        }
    }
}

/// What answers one line of input.
enum Reply {
    One(Response),
    Batch(Vec<Response>),
}

impl Reply {
    fn needs_window(&self) -> bool {
        let is_call = |response: &Response| matches!(response, Response::Call(_));
        match self {
            Self::One(response) => is_call(response),
            Self::Batch(responses) => responses.iter().any(is_call),
        }
    }

    fn resolve(self, caller: &mut impl ToolCaller) -> Value {
        match self {
            Self::One(response) => response.resolve(caller),
            Self::Batch(responses) => responses
                .into_iter()
                .map(|response| response.resolve(caller))
                .collect(),
        }
    }
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|version| PROTOCOL_VERSIONS.contains(version))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "ovrley", "title": "OVRLEY", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn tools_list() -> Value {
    let tools: Vec<Value> = catalog::tools().iter().map(catalog::Tool::listing).collect();
    json!({ "tools": tools })
}

fn tool_call(id: Value, params: &Value) -> Response {
    let refuse = |id, message: String| Response::Ready(error_response(id, INVALID_PARAMS, message));
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return refuse(id, "tools/call needs a tool name".into());
    };
    if catalog::find(name).is_none() {
        return refuse(id, format!("Unknown tool: {name}"));
    }
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    if !(arguments.is_null() || arguments.is_object()) {
        return refuse(id, "Tool arguments must be an object".into());
    }
    Response::Call(ToolCall { id, name: name.to_string(), arguments })
}

/// What answers one message. Notifications, and responses to requests, get no answer.
fn respond(message: &Value) -> Option<Response> {
    let invalid = |reason: &str| {
        Some(Response::Ready(error_response(Value::Null, INVALID_REQUEST, reason)))
    };
    let Value::Object(message) = message else {
        return invalid("Expected a JSON-RPC request object");
    };
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        if message.contains_key("result") || message.contains_key("error") {
            return None;
        }
        return invalid("The request has no method");
    };
    let id = message.get("id")?.clone();
    if !(id.is_string() || id.is_i64() || id.is_u64()) {
        return invalid("The request id must be a string or an integer");
    }
    let params = message.get("params").unwrap_or(&Value::Null);
    let answer = match method {
        "tools/call" => return Some(tool_call(id, params)),
        "initialize" => result_response(id, initialize(params)),
        "ping" => result_response(id, json!({})),
        "tools/list" => result_response(id, tools_list()),
        other => error_response(id, METHOD_NOT_FOUND, format!("Method not found: {other}")),
    };
    Some(Response::Ready(answer))
}

/// What answers one line of input, if anything does.
fn read_message(line: &[u8]) -> Option<Reply> {
    let refuse = |code, reason: String| {
        Some(Reply::One(Response::Ready(error_response(Value::Null, code, reason))))
    };
    match serde_json::from_slice::<Value>(line) {
        Err(error) => refuse(PARSE_ERROR, format!("Parse error: {error}")),
        Ok(Value::Array(batch)) if batch.is_empty() => {
            refuse(INVALID_REQUEST, "The batch is empty".into())
        }
        Ok(Value::Array(batch)) => {
            let responses: Vec<Response> = batch.iter().filter_map(respond).collect();
            (!responses.is_empty()).then_some(Reply::Batch(responses))
        }
        Ok(message) => respond(&message).map(Reply::One),
    }
}

fn write_message(output: &Mutex<impl Write>, message: &Value) -> io::Result<()> {
    let mut output = output.lock().expect("a write to standard output panicked");
    serde_json::to_writer(&mut *output, message)?;
    output.write_all(b"\n")?;
    output.flush()
}

/// Serves MCP until `input` closes and the calls it brought have answered.
///
/// Pass `&mut` references to keep using the reader or the writer.
///
/// # Errors
///
/// When `input` cannot be read or `output` cannot be written.
///
/// # Panics
///
/// When `caller` or `output` panicked on the thread that runs the calls.
pub fn serve(
    mut input: impl BufRead,
    output: impl Write + Send,
    caller: &mut (impl ToolCaller + Send),
) -> io::Result<()> {
    let output = Mutex::new(output);
    std::thread::scope(|scope| {
        let (calls, queued) = mpsc::channel::<Reply>();
        let output = &output;
        let worker = std::thread::Builder::new()
            .name("mcp-calls".into())
            .spawn_scoped(scope, move || -> io::Result<()> {
                for reply in queued {
                    write_message(output, &reply.resolve(caller))?;
                }
                Ok(())
            })?;
        let mut line = Vec::new();
        loop {
            line.clear();
            if input.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            if line.trim_ascii().is_empty() {
                continue;
            }
            match read_message(&line) {
                None => {}
                Some(reply) if reply.needs_window() => {
                    // The worker stops only when it cannot write, and reports that below.
                    if calls.send(reply).is_err() {
                        break;
                    }
                }
                Some(Reply::One(Response::Ready(answer))) => write_message(output, &answer)?,
                Some(reply) => write_message(output, &reply.resolve(&mut NoCalls))?,
            }
        }
        drop(calls);
        worker.join().expect("the thread that runs tool calls panicked")
    })
}

/// The caller for replies that hold no call.
struct NoCalls;

impl ToolCaller for NoCalls {
    fn call(&mut self, tool: &str, _: Value) -> ToolResult {
        unreachable!("{tool} was answered without the window")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{self, NOT_OPEN};
    use crate::testing::{socket, Editor, UiThread};

    impl ToolCaller for Editor {
        fn call(&mut self, tool: &str, arguments: Value) -> ToolResult {
            Editor::call(self, tool, arguments)
        }
    }

    struct NoWindow;

    impl ToolCaller for NoWindow {
        fn call(&mut self, _: &str, _: Value) -> ToolResult {
            Err(ToolError::Failed(NOT_OPEN.into()))
        }
    }

    fn answer(caller: &mut impl ToolCaller, line: &str) -> Option<Value> {
        read_message(line.as_bytes()).map(|reply| reply.resolve(caller))
    }

    fn ask(caller: &mut impl ToolCaller, request: &Value) -> Value {
        answer(caller, &request.to_string()).expect("an answer")
    }

    fn call(name: &str, arguments: &Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments },
        })
    }

    #[test]
    fn initialize_negotiates_the_revision() {
        let initialize = |version: &str| {
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": version },
            })
        };
        let served = ask(&mut NoWindow, &initialize("2025-03-26"));
        assert_eq!(served["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(served["result"]["capabilities"]["tools"]["listChanged"], false);
        let unknown = ask(&mut NoWindow, &initialize("2099-01-01"));
        assert_eq!(unknown["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn tools_list_needs_no_window_and_has_no_references() {
        let list = json!({"jsonrpc": "2.0", "id": "a", "method": "tools/list"});
        let answer = ask(&mut NoWindow, &list);
        let tools = answer["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), catalog::tools().len());
        assert_eq!(tools[0]["name"], "ovrley_summary");
        assert!(!answer.to_string().contains("$ref"));
        assert_eq!(answer["id"], "a");
    }

    #[test]
    fn a_call_runs_in_the_window_and_returns_structured_content() {
        let mut window = Editor::new();
        let answer = ask(&mut window, &call("drawer_select_tool", &json!({"tool": "activity"})));
        let result = &answer["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["drawer"]["activeTool"], "activity");
        assert!(result["content"][0]["text"].as_str().unwrap().contains("\"activity\""));
        assert!(window.dispatcher.with_store(|store| store.layout.left_drawer_visible));
    }

    #[test]
    fn unknown_tools_and_invalid_arguments_are_invalid_params() {
        let mut window = Editor::new();
        for request in [
            call("nope", &json!({})),
            call("language_set", &json!({"language": "xx"})),
            call("drawer_dismiss", &json!([1])),
        ] {
            assert_eq!(ask(&mut window, &request)["error"]["code"], INVALID_PARAMS, "{request}");
        }
    }

    #[test]
    fn edits_off_and_no_window_are_tool_errors() {
        let request = call("ovrley_summary", &json!({}));
        let mut window = Editor::new();
        window.dispatcher.dispatch(|store, _| store.agent_session.allow_agent_edits = false);
        let off = ask(&mut window, &request);
        assert_eq!(off["result"]["isError"], true);
        assert_eq!(off["result"]["content"][0]["text"], catalog::EDITS_OFF);
        let closed = ask(&mut NoWindow, &request);
        assert_eq!(closed["result"]["isError"], true);
        assert_eq!(closed["result"]["content"][0]["text"], NOT_OPEN);
    }

    #[test]
    fn json_rpc_edge_cases() {
        for (line, code) in [
            ("{", PARSE_ERROR),
            ("7", INVALID_REQUEST),
            ("[]", INVALID_REQUEST),
            ("{}", INVALID_REQUEST),
            (r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#, INVALID_REQUEST),
            (r#"{"jsonrpc":"2.0","id":8,"method":"resources/list"}"#, METHOD_NOT_FOUND),
        ] {
            assert_eq!(answer(&mut NoWindow, line).unwrap()["error"]["code"], code, "{line}");
        }
        let batch = r#"[{"jsonrpc":"2.0","id":9,"method":"ping"},{"method":"notifications/x"}]"#;
        assert_eq!(
            answer(&mut NoWindow, batch).unwrap(),
            json!([{"jsonrpc": "2.0", "id": 9, "result": {}}])
        );
        for unanswered in [
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
        ] {
            assert!(answer(&mut NoWindow, unanswered).is_none(), "{unanswered}");
        }
    }

    #[test]
    fn a_waiting_call_holds_up_no_ping_and_bad_bytes_end_nothing() {
        /// Answers its call once the ping was answered.
        struct Waits(mpsc::Receiver<()>);

        impl ToolCaller for Waits {
            fn call(&mut self, _: &str, _: Value) -> ToolResult {
                self.0
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .map(|()| json!({}))
                    .map_err(|error| ToolError::Failed(error.to_string()))
            }
        }

        /// Reports the ping's answer.
        struct Output(Vec<u8>, mpsc::Sender<()>);

        impl Write for Output {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                if self.0.ends_with(b"\"id\":2,\"result\":{}}\n") {
                    self.1.send(()).expect("the call waits");
                }
                Ok(())
            }
        }

        let (pinged, waits) = mpsc::channel();
        let mut input = call("drawer_dismiss", &json!({})).to_string().into_bytes();
        input.extend_from_slice(b"\n\xff\xfe\n");
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n");
        let mut output = Output(Vec::new(), pinged);
        serve(&input[..], &mut output, &mut Waits(waits)).unwrap();
        let lines: Vec<Value> = output
            .0
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        assert_eq!(lines[0]["error"]["code"], PARSE_ERROR);
        assert_eq!(lines[1]["id"], 2);
        assert_eq!(lines[2]["result"]["isError"], false, "the call answered after the ping");
    }

    #[test]
    fn through_the_socket_against_a_dispatcher() {
        let location = socket("protocol");
        let _server = link::serve(location.clone(), UiThread::start()).unwrap().expect("serving");
        let mut client = LinkClient::new(location);
        let answer = ask(&mut client, &call("language_set", &json!({"language": "fr"})));
        assert_eq!(answer["result"]["structuredContent"]["language"], "fr");
    }
}

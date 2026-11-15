//! MCP over stdio for the `ovrley --mcp` process (D13; `concat-mcp-protocol.md`).
//!
//! - **Framing:** newline-delimited JSON-RPC 2.0. Standard output carries only protocol messages;
//!   logs go to standard error.
//! - **Methods:** `initialize`, `ping`, `tools/list` and `tools/call`. `notifications/initialized`
//!   and `notifications/cancelled` are accepted and need no answer: calls are short and run one at a
//!   time, so there is nothing to cancel.
//! - **Versions:** an `initialize` asking for a revision this server serves gets it back; any other
//!   gets the newest.
//! - **Answers:** this process answers `initialize` and `tools/list` itself, from the compiled-in
//!   catalog, and forwards `tools/call` to the window. A tool that ran and failed is a result with
//!   `isError`; malformed arguments and unknown tools are invalid-params errors.

use crate::catalog::{self, ToolOutcome};
use serde_json::{json, Map, Value};
use std::io::{BufRead, Write};

/// The revisions this server serves, newest first.
pub const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

const INSTRUCTIONS: &str = "OVRLEY turns activity data into video overlays. These tools drive the OVRLEY window that is open on \
     this computer, as the person at it would. Start with ovrley_summary to see its state. The person must turn on \
     \"Allow agent edits\" in OVRLEY's Settings first.";

/// Runs a tool somewhere: in the window, through [`crate::link::LinkClient`], or in tests.
pub trait ToolCaller {
    fn call(&mut self, tool: &str, arguments: Value) -> ToolOutcome;
}

impl ToolCaller for crate::link::LinkClient {
    fn call(&mut self, tool: &str, arguments: Value) -> ToolOutcome {
        crate::link::LinkClient::call(self, tool, arguments)
    }
}

fn error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
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
    let tools: Vec<Value> = catalog::tools()
        .into_iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.description,
                "inputSchema": tool.input_schema,
                "annotations": {
                    "readOnlyHint": tool.read_only,
                    "destructiveHint": false,
                    "openWorldHint": false,
                },
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn tools_call(id: Value, params: &Value, caller: &mut dyn ToolCaller) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error(id, INVALID_PARAMS, "tools/call needs a tool name");
    };
    if !catalog::is_tool(name) {
        return error(id, INVALID_PARAMS, format!("Unknown tool: {name}"));
    }
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    if !(arguments.is_null() || arguments.is_object()) {
        return error(id, INVALID_PARAMS, "Tool arguments must be an object");
    }
    match caller.call(name, arguments) {
        ToolOutcome::Ok(value) => {
            let text = serde_json::to_string_pretty(&value).expect("a tool result serializes");
            let mut answer = Map::new();
            answer.insert("content".into(), json!([{ "type": "text", "text": text }]));
            if value.is_object() {
                answer.insert("structuredContent".into(), value);
            }
            answer.insert("isError".into(), Value::Bool(false));
            result(id, Value::Object(answer))
        }
        ToolOutcome::InvalidParams(message) => error(id, INVALID_PARAMS, message),
        ToolOutcome::Failed(message) => result(
            id,
            json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
        ),
    }
}

/// Answers one line of input. Notifications get no answer.
pub fn handle_line(line: &str, caller: &mut dyn ToolCaller) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(parse) => {
            return Some(error(
                Value::Null,
                PARSE_ERROR,
                format!("Parse error: {parse}"),
            ))
        }
    };
    let Value::Object(message) = message else {
        return Some(error(
            Value::Null,
            INVALID_REQUEST,
            "Expected one JSON-RPC request object",
        ));
    };
    let method = message.get("method").and_then(Value::as_str);
    let Some(id) = message.get("id").cloned() else {
        // A notification, or a response this server never asked for.
        return None;
    };
    if !(id.is_string() || id.is_i64() || id.is_u64()) {
        return Some(error(
            Value::Null,
            INVALID_REQUEST,
            "The request id must be a string or an integer",
        ));
    }
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        Some("initialize") => result(id, initialize(&params)),
        Some("ping") => result(id, json!({})),
        Some("tools/list") => result(id, tools_list()),
        Some("tools/call") => tools_call(id, &params, caller),
        Some(other) => error(id, METHOD_NOT_FOUND, format!("Method not found: {other}")),
        None => error(id, INVALID_REQUEST, "The request has no method"),
    })
}

/// The `--mcp` process: reads requests from standard input until it closes, and answers on
/// standard output.
pub fn run_stdio(caller: &mut dyn ToolCaller) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = handle_line(&line, caller) {
            serde_json::to_writer(&mut stdout, &answer)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs tools in a dispatcher over fake services, as the window would.
    struct InProcess {
        dispatcher: ovrley_app::store::Dispatcher,
    }

    impl InProcess {
        fn new(allowed: bool) -> Self {
            let (parts, _) = ovrley_app::store::services::fakes::fake_services(
                json!({ "allowAgentEdits": allowed }),
            );
            let store = ovrley_app::store::Store::hydrate(&parts.preferences).unwrap();
            Self {
                dispatcher: ovrley_app::store::Dispatcher::new(store, parts),
            }
        }
    }

    impl ToolCaller for InProcess {
        fn call(&mut self, tool: &str, arguments: Value) -> ToolOutcome {
            let tool = tool.to_string();
            // These tests call tools that answer in their dispatch.
            let answer = self
                .dispatcher
                .dispatch_with_result(move |store, services| {
                    catalog::call(&tool, arguments, store, services, Box::new(|_| panic!("answered later")))
                })
                .unwrap();
            match answer {
                catalog::Answer::Now(outcome) => outcome,
                catalog::Answer::Later => panic!("the protocol tests call tools that answer at once"),
            }
        }
    }

    /// No window.
    struct NoWindow;

    impl ToolCaller for NoWindow {
        fn call(&mut self, _: &str, _: Value) -> ToolOutcome {
            ToolOutcome::Failed(crate::link::NOT_OPEN.into())
        }
    }

    fn ask(caller: &mut dyn ToolCaller, request: Value) -> Value {
        handle_line(&request.to_string(), caller).expect("an answer")
    }

    #[test]
    fn initialize_negotiates_the_revision() {
        let answer = ask(
            &mut NoWindow,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26"}}),
        );
        assert_eq!(answer["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(
            answer["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        let answer = ask(
            &mut NoWindow,
            json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "2099-01-01"}}),
        );
        assert_eq!(answer["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn tools_list_needs_no_window_and_has_no_references() {
        let answer = ask(
            &mut NoWindow,
            json!({"jsonrpc": "2.0", "id": "a", "method": "tools/list"}),
        );
        let tools = answer["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        let expected: Vec<&str> = catalog::tools().iter().map(|tool| tool.name).collect();
        assert_eq!(names, expected);
        assert!(names.starts_with(&["ovrley_summary", "language_set", "drawer_select_tool", "drawer_set_pinned", "drawer_dismiss"]));
        assert!(!answer.to_string().contains("$ref"));
        assert_eq!(answer["id"], "a");
    }

    #[test]
    fn a_call_runs_in_the_window_and_returns_structured_content() {
        let mut window = InProcess::new(true);
        let answer = ask(
            &mut window,
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "drawer_select_tool", "arguments": {"tool": "activity"}}}),
        );
        assert_eq!(answer["result"]["isError"], false);
        assert_eq!(
            answer["result"]["structuredContent"]["drawer"]["activeTool"],
            "activity"
        );
        assert!(answer["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("\"activity\""));
        assert!(window
            .dispatcher
            .with_store(|store| store.layout.left_drawer_visible));
    }

    #[test]
    fn unknown_tools_and_invalid_arguments_are_invalid_params() {
        let mut window = InProcess::new(true);
        let unknown = ask(
            &mut window,
            json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "nope"}}),
        );
        assert_eq!(unknown["error"]["code"], INVALID_PARAMS);
        let invalid = ask(
            &mut window,
            json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "language_set", "arguments": {"language": "xx"}}}),
        );
        assert_eq!(invalid["error"]["code"], INVALID_PARAMS);
        let not_object = ask(
            &mut window,
            json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "drawer_dismiss", "arguments": [1]}}),
        );
        assert_eq!(not_object["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn edits_off_and_no_window_are_tool_errors() {
        let call = json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "ovrley_summary"}});
        let off = ask(&mut InProcess::new(false), call.clone());
        assert_eq!(off["result"]["isError"], true);
        assert_eq!(off["result"]["content"][0]["text"], catalog::EDITS_OFF);
        let closed = ask(&mut NoWindow, call);
        assert_eq!(closed["result"]["isError"], true);
        assert_eq!(
            closed["result"]["content"][0]["text"],
            crate::link::NOT_OPEN
        );
    }

    #[test]
    fn json_rpc_edge_cases() {
        assert_eq!(
            handle_line("{", &mut NoWindow).unwrap()["error"]["code"],
            PARSE_ERROR
        );
        assert_eq!(
            handle_line("[1, 2]", &mut NoWindow).unwrap()["error"]["code"],
            INVALID_REQUEST
        );
        assert_eq!(
            ask(
                &mut NoWindow,
                json!({"jsonrpc": "2.0", "id": null, "method": "ping"})
            )["error"]["code"],
            INVALID_REQUEST
        );
        assert_eq!(
            ask(
                &mut NoWindow,
                json!({"jsonrpc": "2.0", "id": 8, "method": "resources/list"})
            )["error"]["code"],
            METHOD_NOT_FOUND
        );
        assert_eq!(
            ask(
                &mut NoWindow,
                json!({"jsonrpc": "2.0", "id": 9, "method": "ping"})
            )["result"],
            json!({})
        );
        assert!(handle_line(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
            &mut NoWindow
        )
        .is_none());
        assert!(handle_line(&json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 3}}).to_string(), &mut NoWindow).is_none());
    }

    #[test]
    fn through_the_socket_against_a_dispatcher() {
        use crate::link::{serve, BridgeHooks, LinkClient, SocketLocation};
        use std::sync::mpsc;
        use std::sync::Arc;

        // The window's UI thread, here a plain thread that owns the dispatcher.
        let (calls, received) = mpsc::channel::<(String, Value, mpsc::Sender<ToolOutcome>)>();
        std::thread::spawn(move || {
            let mut window = InProcess::new(true);
            for (tool, arguments, reply) in received {
                let _ = reply.send(ToolCaller::call(&mut window, &tool, arguments));
            }
        });
        let calls = std::sync::Mutex::new(calls);
        let hooks = BridgeHooks {
            call: Arc::new(move |tool, arguments| {
                let (reply, answer) = mpsc::channel();
                calls
                    .lock()
                    .unwrap()
                    .send((tool, arguments, reply))
                    .unwrap();
                answer.recv().unwrap()
            }),
            session: Arc::new(|_| {}),
        };
        let location = if cfg!(windows) {
            SocketLocation::Namespaced(format!("ovrley-mcp-test-protocol-{}", std::process::id()))
        } else {
            SocketLocation::File(
                std::env::temp_dir()
                    .join(format!("om-proto-{}", std::process::id()))
                    .join("s.sock"),
            )
        };
        let _server = serve(location.clone(), hooks).unwrap().unwrap();
        let mut client = LinkClient::new(location);
        let answer = ask(
            &mut client,
            json!({"jsonrpc": "2.0", "id": 10, "method": "tools/call", "params": {"name": "language_set", "arguments": {"language": "fr"}}}),
        );
        assert_eq!(answer["result"]["structuredContent"]["language"], "fr");
    }
}

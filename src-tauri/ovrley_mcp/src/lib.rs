//! OVRLEY's MCP server (D13). The `ovrley --mcp` process speaks MCP over stdio (`protocol`) and
//! forwards tool calls over a local socket (`link`) to the running window, where they run on the UI
//! thread against the store (`catalog`). No async runtime: std threads only.

pub mod catalog;
pub mod link;
pub mod player;
pub mod protocol;
pub mod scene_settings;
pub mod widgets;

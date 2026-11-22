//! Deferred replies to agent tool calls (Phase 2 D13, Phase 3 D15).
//!
//! A tool call is one dispatch. When its result is not known by the end of that dispatch (it waits
//! for a worker or for the user), the action keeps an [`AgentReply`] and answers later, on the UI
//! thread. Dropping a reply unanswered answers that the call was cancelled.

use super::slices::media::ActivitySource;
use super::Store;
use serde_json::{json, Value};

/// A tool's result: its value, or the error the agent reads.
pub type AgentResult = Result<Value, String>;

/// What an agent reads when the call it waits for is dropped without an answer.
pub const CANCELLED: &str = "The request was cancelled before OVRLEY could answer.";

/// A path an agent gave, which is absolute: checked when the arguments are read.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "String")]
pub struct AbsolutePath(String);

impl TryFrom<String> for AbsolutePath {
    type Error = String;

    fn try_from(path: String) -> Result<Self, Self::Error> {
        if std::path::Path::new(&path).is_absolute() {
            Ok(Self(path))
        } else {
            Err(format!("The path must be absolute: {path}"))
        }
    }
}

impl From<AbsolutePath> for String {
    fn from(path: AbsolutePath) -> Self {
        path.0
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for AbsolutePath {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        String::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        String::json_schema(generator)
    }
}

/// Answers one tool call, once.
pub struct AgentReply(Option<Box<dyn FnOnce(AgentResult) + Send>>);

impl AgentReply {
    pub fn new(reply: impl FnOnce(AgentResult) + Send + 'static) -> Self {
        Self(Some(Box::new(reply)))
    }

    pub fn send(mut self, result: AgentResult) {
        if let Some(reply) = self.0.take() {
            reply(result);
        }
    }
}

impl Drop for AgentReply {
    fn drop(&mut self) {
        if let Some(reply) = self.0.take() {
            reply(Err(CANCELLED.into()));
        }
    }
}

impl std::fmt::Debug for AgentReply {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AgentReply")
    }
}

/// The open activity, as agents read it: its file, format, duration and laps; `null` without one.
pub fn describe_activity(store: &Store) -> Value {
    let Some(summary) = &store.media.activity_summary else { return Value::Null };
    let file = match &store.media.activity_source {
        Some(ActivitySource::File { path }) => Value::String(path.clone()),
        None => Value::Null,
    };
    let laps: Vec<Value> = summary
        .laps
        .iter()
        .map(|lap| {
            json!({
                "id": lap.id,
                "startSeconds": lap.start_seconds,
                "durationSeconds": lap.duration_seconds,
                "enabled": !store.media.disabled_laps.contains(&lap.id),
            })
        })
        .collect();
    json!({
        "file": file,
        "fileName": summary.file_name,
        "format": summary.file_format,
        "durationSeconds": summary.duration_seconds,
        "laps": laps,
        "disabledLaps": store.media.disabled_laps,
    })
}

/// The loaded template, as agents read it: its source, its name and its save status.
pub fn describe_template(store: &Store) -> Value {
    use super::slices::template::TemplateSource;
    let (source, name) = match &store.template.loaded_template_source {
        Some(TemplateSource::Bundled { template_id }) => (json!({"kind": "bundled", "templateId": template_id}), Some(template_id.clone())),
        Some(TemplateSource::File { path }) => (json!({"kind": "file", "path": path}), Some(crate::features::template_manager::filename_from_path(Some(path)))),
        None => (Value::Null, None),
    };
    json!({
        "source": source,
        "name": name,
        "saveStatus": crate::features::template_manager::save_status(store).name(),
    })
}

/// The document's widgets, as agents read them: id, type, name, position and whether selected.
pub fn describe_widgets(store: &Store) -> Value {
    let widgets: Vec<Value> = crate::lib::widget::build_config_widgets(&store.editor.config)
        .into_iter()
        .map(|widget| {
            json!({
                "id": widget.id,
                "type": widget.kind,
                "name": crate::lib::widget::group_widgets_for_sidebar(std::slice::from_ref(&widget), crate::i18n::Locale::En)[0].name,
                "x": widget.data.get("x"),
                "y": widget.data.get("y"),
                "selected": store.editor.selected_widget_ids.contains(&widget.id),
            })
        })
        .collect();
    Value::Array(widgets)
}

/// One widget for `widget_get`: its durable data, its presented content rectangle, and the fields
/// its type takes (the catalog's config keys).
pub fn describe_widget(store: &Store, widget_id: &str) -> Result<Value, String> {
    let data = store.editor.config.widget(widget_id)?;
    let (collection, _) = store.editor.config.find_widget(widget_id)?;
    let kind = collection.widget_type(data)?;
    let definition = ovrley_widgets::model::catalog::catalog().require_widget(&kind)?;
    let content = crate::features::overlay_editor::interaction::content_rect(store, widget_id)
        .map(|rect| json!({"left": rect.left, "top": rect.top, "width": rect.width, "height": rect.height}));
    Ok(json!({
        "id": widget_id,
        "type": kind,
        "data": data,
        "content": content,
        "fields": definition.config_keys,
        "displayTypes": definition.display_types,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_reply_answers_once_and_a_dropped_reply_cancels() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        AgentReply::new(move |result| log.lock().unwrap().push(result)).send(Ok(Value::Bool(true)));
        let log = seen.clone();
        drop(AgentReply::new(move |result| log.lock().unwrap().push(result)));
        assert_eq!(*seen.lock().unwrap(), [Ok(Value::Bool(true)), Err(CANCELLED.to_string())]);
    }
}

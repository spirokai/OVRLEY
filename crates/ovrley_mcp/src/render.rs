//! The render tools: the options, start, progress and cancel, as the render dialog offers them.
//! The window shows what an agent does: a start opens the dialog, which the user can cancel.

use crate::catalog::{Answer, NoArguments, Tool, ToolError, ToolResult};
use ovrley_app::features::render::agent::{self, AgentRenderOptions};
use ovrley_app::store::agent::AgentReply;
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use schemars::JsonSchema;
use serde::Deserialize;

/// The longest `render_progress` waits. Each wait holds a worker thread for that long.
const MAX_WAIT_SECONDS: f64 = 600.0;

/// How long to wait.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ProgressArguments {
    /// Seconds to wait for the render to end before answering (0–600, default 0).
    #[schemars(range(min = 0, max = 600))]
    wait_seconds: Option<f64>,
}

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool::read(
            "render_options",
            "Render options",
            "What the render dialog offers now: the formats and their accelerations with their \
             availability on this machine, the defaults (the committed settings), the locked \
             frame rate, the widget update rates, the quality and bitrate ranges, whether output \
             modes and renderFullActivity apply, and what blocks a render (blocker, busy).",
            options,
        ),
        Tool::waiting(
            "render_start",
            "Start a render",
            "Renders as the render dialog's Start does: the dialog opens in the window with the \
             settings, unset ones keep the dialog's defaults. Without videos it renders the \
             activity overlay; with videos it waits for their review and renders one combined \
             file or one file per video. Fails with the reason where Start would be disabled. \
             Without overwrite, an existing output file opens the overwrite question and the call \
             waits for the user ({started: false} when they cancel). Replies on acceptance with \
             the target, the render or batch id and the outputs; follow it with render_progress. \
             The accepted settings are committed, outside the undo history.",
            start,
        )
        .destructive(),
        Tool::waiting(
            "render_progress",
            "Render progress",
            "The current or last render: target, status, percent, frames, render fps, seconds \
             remaining; for a video export each video with its outcome and the output paths; the \
             error. With waitSeconds the reply waits until the render ends or the time is up, \
             whichever comes first; the user can still cancel in the window.",
            progress,
        )
        .read_only(),
        Tool::edit(
            "render_cancel",
            "Cancel the render",
            "Cancels the running render, single or video export. Replies once cancellation is \
             requested; render_progress shows when the cleanup ended.",
            cancel,
        )
        .destructive(),
    ]
}

fn options(_: NoArguments, store: &Store, _: &Services) -> ToolResult {
    Ok(agent::render_options(store))
}

fn start(
    options: AgentRenderOptions,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    agent::agent_start(store, services, options, reply);
    Ok(Answer::Later)
}

fn progress(
    args: ProgressArguments,
    store: &mut Store,
    services: &mut Services,
    reply: AgentReply,
) -> Result<Answer, ToolError> {
    let wait = args.wait_seconds.unwrap_or(0.0);
    if !(0.0..=MAX_WAIT_SECONDS).contains(&wait) {
        return Err(ToolError::InvalidParams(format!(
            "waitSeconds must be between 0 and {MAX_WAIT_SECONDS}"
        )));
    }
    agent::agent_progress(store, services, wait, reply);
    Ok(Answer::Later)
}

fn cancel(_: NoArguments, store: &mut Store, services: &mut Services) -> ToolResult {
    Ok(agent::agent_cancel(store, services)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Editor, Started};
    use ovrley_app::features::render;
    use ovrley_app::features::player::filmstrips::CodecDetection;
    use ovrley_app::store::services::fakes::FakeRenderCall;
    use ovrley_app::store::media::ImportedActivity;
    use ovrley_core::debug::{RenderProgress, RenderStatus};
    use ovrley_core::encode::progress::ProgressSink;
    use ovrley_core::error::CoreError;
    use ovrley_widgets::model::document::OverlayConfig;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn editor() -> Editor {
        let mut store = Store::default();
        let document = json!({"scene": {"width": 1920, "height": 1080, "fps": 30}});
        store.editor.config = std::rc::Rc::new(OverlayConfig::from_value(&document).unwrap());
        store.media.activate_activity_file(ImportedActivity {
            activity: Arc::new(ovrley_core::activity::schema::ParsedActivity {
                sample_elapsed_seconds: vec![0.0, 10.0, 20.0],
                trim_end_seconds: 20.0,
                ..Default::default()
            }),
            preview_bytes: Arc::new(Vec::new()),
        });
        store.video_import.available_codecs =
            CodecDetection::Done(Some(ovrley_core::encode::ffmpeg::detect::AvailableCodecs {
                prores_ks: true,
                qtrle: true,
                ..Default::default()
            }));
        Editor::with(store, json!({"last-render-output-dir": "/renders"}))
    }

    /// Starts a render whose output exists, and returns once the dialog asks about it.
    fn start_over_existing_output(editor: &Editor) -> Started {
        editor
            .fakes
            .render
            .single_results
            .lock()
            .unwrap()
            .push_back(Err(CoreError::OutputExists("exists".into())));
        let waiting = editor.start("render_start", json!({}));
        assert!(waiting.answer.is_none());
        editor.until("the overwrite question", || {
            editor
                .dispatcher
                .with_store(|store| store.render_dialog.pending_overwrite.as_ref().map(|_| ()))
        });
        assert!(
            waiting.reply.try_recv().is_err(),
            "the call waits for the user"
        );
        waiting
    }

    #[test]
    fn options_list_formats_with_availability_and_the_defaults() {
        let options = editor().ok("render_options", json!({}));
        assert_eq!(options["defaults"]["exportMode"], "transparent");
        assert_eq!(options["formats"][0]["format"], "prores");
        assert_eq!(
            options["formats"][2]["available"], true,
            "software H.264 once detection ended"
        );
        assert_eq!(options["blocker"], Value::Null);
        assert_eq!(options["widgetUpdateRates"], json!([1, 2, 3, 5]));
    }

    #[test]
    fn a_start_opens_the_dialog_renders_and_is_followed_to_its_end() {
        let editor = editor();
        let started = editor.ok(
            "render_start",
            json!({"format": "qtrle", "fps": 24, "widgetUpdateRate": 2}),
        );
        assert_eq!(
            (&started["target"], &started["outputPath"]),
            (&json!("single"), &json!("/renders/overlay_1.mov"))
        );
        editor.dispatcher.with_store(|store| {
            let settings = &store.render_settings;
            assert_eq!(
                settings.codec, "qtrle",
                "the accepted settings are committed"
            );
            assert_eq!((settings.fps, settings.widget_update_rate), (24.0, 2));
            assert!(store.render_dialog.is_open(), "the window shows the render");
        });
        // A second start while busy fails.
        editor.failed("render_start", json!({}));
        assert_eq!(
            editor.ok("ovrley_summary", json!({}))["render"]["busy"],
            true
        );
        // The reply comes when the render ends.
        let waiting = editor.start("render_progress", json!({"waitSeconds": 60}));
        assert!(waiting.answer.is_none());
        editor.fakes.render.mailbox.emit_progress(&RenderProgress {
            render_id: started["renderId"].as_u64().unwrap(),
            current: 10,
            total: 10,
            status: RenderStatus::Complete,
            filename: Some("overlay_1.mov".into()),
            ..RenderProgress::default()
        });
        let progress = editor.finish(waiting).unwrap();
        assert_eq!(
            (
                &progress["status"],
                &progress["finished"],
                &progress["percent"]
            ),
            (&json!("complete"), &json!(true), &json!(100.0))
        );
    }

    #[test]
    fn an_existing_output_asks_the_user_and_the_call_waits_for_the_answer() {
        let editor = editor();
        let waiting = start_over_existing_output(&editor);
        editor
            .dispatcher
            .dispatch(|store, _| render::cancel_overwrite(store));
        assert_eq!(editor.finish(waiting), Ok(json!({"started": false})));

        // Answered: the overwrite renders.
        editor
            .dispatcher
            .dispatch(|store, _| render::close_render_dialog(store));
        let waiting = start_over_existing_output(&editor);
        editor.dispatcher.dispatch(render::confirm_overwrite);
        assert_eq!(editor.finish(waiting).unwrap()["started"], true);
        assert!(editor.fakes.render.calls().iter().any(|call| matches!(
            call,
            FakeRenderCall::SubmitSingle {
                overwrite: true,
                ..
            }
        )));
    }

    #[test]
    fn the_question_dropped_with_the_window_cancels_the_call() {
        let editor = editor();
        let waiting = start_over_existing_output(&editor);
        editor
            .dispatcher
            .dispatch(|store, _| render::close_render_dialog(store));
        assert_eq!(
            editor.finish(waiting),
            Err(ToolError::Failed(
                ovrley_app::store::agent::CANCELLED.into()
            ))
        );
    }

    #[test]
    fn progress_answers_at_the_deadline_and_cancel_stops_the_render() {
        let editor = editor();
        // Nothing runs.
        editor.failed("render_cancel", json!({}));
        editor.ok("render_start", json!({}));
        let started = Instant::now();
        let progress = editor.ok("render_progress", json!({"waitSeconds": 0.05}));
        assert!(started.elapsed() >= Duration::from_millis(50));
        assert_eq!(progress["busy"], true);
        assert_eq!(
            editor.ok("render_cancel", json!({})),
            json!({"cancelling": true})
        );
        assert!(editor
            .fakes
            .render
            .calls()
            .contains(&FakeRenderCall::Cancel));
        editor.invalid("render_progress", json!({"waitSeconds": 601}));
    }

    #[test]
    fn a_start_fails_with_the_reason_start_would_be_disabled() {
        let editor = editor();
        assert_eq!(
            editor.failed("render_start", json!({"exportMode": "composite"})),
            "Full video export needs an imported video."
        );
        assert!(
            !editor
                .dispatcher
                .with_store(|store| store.render_dialog.is_open()),
            "a failed start closes the dialog"
        );
        let reason = editor.failed("render_start", json!({"widgetUpdateRate": 7}));
        assert!(reason.contains("widget update rate"), "{reason}");
        let reason = editor.invalid("render_start", json!({"outputPath": "relative.mov"}));
        assert!(reason.contains("absolute"), "{reason}");
        editor
            .dispatcher
            .dispatch(|store, _| store.media.clear_activity_file(true));
        assert_eq!(
            editor.failed("render_start", json!({})),
            "Load a GPX/FIT activity first"
        );
    }
}

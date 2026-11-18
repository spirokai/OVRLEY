//! The render tools of Phase 7 (D10): the options, start, progress and cancel, as the render dialog
//! offers them. The window shows what an agent does: `render_start` opens the dialog, which a user
//! can cancel. The catalog lists these after its own and hands them their calls.

use crate::catalog::{input_schema, Answer, Reply, ToolDefinition, ToolOutcome};
use ovrley_app::features::render_video::agent::{self, AgentRenderOptions};
use ovrley_app::features::render_video::codecs::{Acceleration, OutputFormat};
use ovrley_app::store::agent::AgentReply;
use ovrley_app::store::services::Services;
use ovrley_app::store::Store;
use ovrley_core::encode::quality::QualityType;
use ovrley_core::encode::render_settings::{ExportMode, OutputMode};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

/// No arguments.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderNoArguments {}

/// A render's settings; unset ones take the dialog's defaults (the committed settings, then the
/// dialog's rules).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RenderStartArguments {
    /// The output format: prores or qtrle (transparent), h264 or hevc (full video).
    pub format: Option<OutputFormat>,
    /// The encoder's acceleration; render_options lists the available ones per format.
    pub acceleration: Option<Acceleration>,
    /// transparent (the overlay alone) or composite (over the videos; needs videos).
    pub export_mode: Option<ExportMode>,
    /// combined (one file) or separate (one file per video; needs more than one video).
    pub output_mode: Option<OutputMode>,
    /// The frame rate; locked to the videos' rate for a full video export.
    pub fps: Option<u32>,
    /// The widget update rate divisor, one of render_options' widgetUpdateRates.
    pub widget_update_rate: Option<u32>,
    /// MP4 rate control: quality (a CRF) or bitrate (Mbps).
    pub quality_type: Option<QualityType>,
    /// The CRF (1–51) or the bitrate in Mbps.
    pub quality_value: Option<f64>,
    /// Render the whole external activity as one transparent file, whatever the videos cover.
    pub render_full_activity: Option<bool>,
    /// The output file, absolute; default: the suggested file in the remembered folder.
    pub output_path: Option<String>,
    /// The folder of separate files, absolute; default: the saved folder. It is saved.
    pub output_folder: Option<String>,
    /// Replace an existing output file. Without it, OVRLEY asks the user and the call waits.
    pub overwrite: Option<bool>,
}

/// How long to wait.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RenderProgressArguments {
    /// Seconds to wait for the render to end before answering (0–600, default 0).
    #[schemars(range(min = 0, max = 600))]
    pub wait_seconds: Option<f64>,
}

pub fn tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "render_options",
            title: "Render options",
            description: "What the render dialog offers now: the formats and their accelerations with their availability on this machine, \
                          the defaults (the committed settings), the locked frame rate, the widget update rates, the quality and bitrate \
                          ranges, whether output modes and renderFullActivity apply, and what blocks a render (blocker, busy).",
            input_schema: input_schema::<RenderNoArguments>(),
            read_only: true,
        },
        ToolDefinition {
            name: "render_start",
            title: "Start a render",
            description: "Renders as the render dialog's Start does: the dialog opens in the window with the settings, unset ones keep the \
                          dialog's defaults. Without videos it renders the activity overlay; with videos it waits for their review and \
                          renders one combined file or one file per video. Fails with the reason where Start would be disabled. Without \
                          overwrite, an existing output file opens the overwrite question and the call waits for the user ({started: \
                          false} when they cancel). Replies on acceptance with the target, the render or batch id and the outputs; follow \
                          it with render_progress. The accepted settings are committed, outside the undo history.",
            input_schema: input_schema::<RenderStartArguments>(),
            read_only: false,
        },
        ToolDefinition {
            name: "render_progress",
            title: "Render progress",
            description: "The current or last render: target, status, percent, frames, render fps, seconds remaining; for a video export each \
                          video with its outcome and the output paths; the error. With waitSeconds the reply waits until the render ends \
                          or the time is up, whichever comes first; the user can still cancel in the window.",
            input_schema: input_schema::<RenderProgressArguments>(),
            read_only: true,
        },
        ToolDefinition {
            name: "render_cancel",
            title: "Cancel the render",
            description: "Cancels the running render, single or video export. Replies once cancellation is requested; render_progress shows \
                          when the cleanup ended.",
            input_schema: input_schema::<RenderNoArguments>(),
            read_only: false,
        },
    ]
}

/// Whether `name` is one of these tools.
pub fn handles(name: &str) -> bool {
    tools().iter().any(|tool| tool.name == name)
}

fn arguments<T: DeserializeOwned>(value: Value) -> Result<T, ToolOutcome> {
    let value = if value.is_null() { json!({}) } else { value };
    serde_json::from_value(value)
        .map_err(|error| ToolOutcome::InvalidParams(format!("Invalid arguments: {error}")))
}

fn agent_reply(reply: Reply) -> AgentReply {
    AgentReply::new(move |result| reply(result.into()))
}

pub fn call(
    name: &str,
    arguments_value: Value,
    store: &mut Store,
    services: &mut Services,
    reply: Reply,
) -> Result<Answer, ToolOutcome> {
    match name {
        "render_options" => {
            arguments::<RenderNoArguments>(arguments_value)?;
            Ok(Answer::Now(ToolOutcome::Ok(agent::render_options(store))))
        }
        "render_start" => {
            let arguments: RenderStartArguments = arguments(arguments_value)?;
            let options = AgentRenderOptions {
                format: arguments.format,
                acceleration: arguments.acceleration,
                export_mode: arguments.export_mode,
                output_mode: arguments.output_mode,
                fps: arguments.fps,
                widget_update_rate: arguments.widget_update_rate,
                quality_type: arguments.quality_type,
                quality_value: arguments.quality_value,
                render_full_activity: arguments.render_full_activity,
                output_path: arguments.output_path,
                output_folder: arguments.output_folder,
                overwrite: arguments.overwrite.unwrap_or(false),
            };
            agent::agent_start(store, services, options, agent_reply(reply));
            Ok(Answer::Later)
        }
        "render_progress" => {
            let arguments: RenderProgressArguments = arguments(arguments_value)?;
            let wait = arguments.wait_seconds.unwrap_or(0.0);
            if !(0.0..=600.0).contains(&wait) {
                return Err(ToolOutcome::InvalidParams(
                    "waitSeconds must be between 0 and 600".into(),
                ));
            }
            agent::agent_progress(store, services, wait, agent_reply(reply));
            Ok(Answer::Later)
        }
        "render_cancel" => {
            arguments::<RenderNoArguments>(arguments_value)?;
            Ok(Answer::Now(agent::agent_cancel(store, services).into()))
        }
        other => Err(ToolOutcome::InvalidParams(format!("Unknown tool: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::call as run_tool;
    use ovrley_app::store::services::fakes::{fake_services, FakeRenderCall, Fakes};
    use ovrley_app::store::Dispatcher;
    use ovrley_core::debug::{RenderProgress, RenderStatus};
    use ovrley_core::encode::progress::ProgressSink;
    use ovrley_core::error::CoreError;
    use std::sync::mpsc::{channel, Receiver};
    use std::time::{Duration, Instant};

    fn dispatcher() -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(json!({"last-render-output-dir": "/renders"}));
        let mut store = Store::default();
        store.agent_session.allow_agent_edits = true;
        store.theme = Some(ovrley_app::lib::template::ThemeColors {
            ice: "#010101".into(),
            aqua: "#020202".into(),
            teal: "#030303".into(),
            accent: "#040404".into(),
        });
        store.editor.config = std::rc::Rc::new(
            ovrley_widgets::model::document::OverlayConfig::from_value(
                &json!({"scene": {"width": 1920, "height": 1080, "fps": 30}}),
            )
            .unwrap(),
        );
        store
            .media
            .activate_activity_file(ovrley_app::store::slices::media::ImportedActivity {
                activity: std::sync::Arc::new(ovrley_core::activity::schema::ParsedActivity {
                    sample_elapsed_seconds: vec![0.0, 10.0, 20.0],
                    trim_end_seconds: 20.0,
                    ..Default::default()
                }),
                preview_bytes: std::sync::Arc::new(Vec::new()),
            });
        store.video_import.available_codecs =
            ovrley_app::features::video_preview::filmstrips::CodecDetection::Done(Some(
                ovrley_core::encode::ffmpeg::detect::AvailableCodecs {
                    prores_ks: true,
                    qtrle: true,
                    ..Default::default()
                },
            ));
        (Dispatcher::new(store, parts), fakes)
    }

    fn start(
        dispatcher: &Dispatcher,
        name: &'static str,
        arguments: Value,
    ) -> (Option<ToolOutcome>, Receiver<ToolOutcome>) {
        let (sender, receiver) = channel();
        let answer = dispatcher
            .dispatch_with_result(move |store, services| {
                run_tool(
                    name,
                    arguments,
                    store,
                    services,
                    Box::new(move |outcome| sender.send(outcome).unwrap()),
                )
            })
            .unwrap();
        (
            match answer {
                Answer::Now(outcome) => Some(outcome),
                Answer::Later => None,
            },
            receiver,
        )
    }

    fn settle(
        dispatcher: &Dispatcher,
        fakes: &Fakes,
        receiver: &Receiver<ToolOutcome>,
    ) -> ToolOutcome {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fakes.ui_post.drain(dispatcher);
            if let Ok(outcome) = receiver.try_recv() {
                return outcome;
            }
            assert!(Instant::now() < deadline, "the tool never answered");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn run(
        dispatcher: &Dispatcher,
        fakes: &Fakes,
        name: &'static str,
        arguments: Value,
    ) -> ToolOutcome {
        let (now, receiver) = start(dispatcher, name, arguments);
        now.unwrap_or_else(|| settle(dispatcher, fakes, &receiver))
    }

    #[test]
    fn options_list_formats_with_availability_and_the_defaults() {
        let (dispatcher, fakes) = dispatcher();
        let ToolOutcome::Ok(options) = run(&dispatcher, &fakes, "render_options", json!({})) else {
            panic!()
        };
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
        let (dispatcher, fakes) = dispatcher();
        let ToolOutcome::Ok(started) = run(
            &dispatcher,
            &fakes,
            "render_start",
            json!({"format": "qtrle", "fps": 24, "widgetUpdateRate": 2}),
        ) else {
            panic!("the start failed")
        };
        assert_eq!(
            (started["target"].clone(), started["outputPath"].clone()),
            (json!("single"), json!("/renders/overlay_1.mov"))
        );
        dispatcher.with_store(|store| {
            assert_eq!(
                store.render_settings.codec, "qtrle",
                "the accepted settings are committed"
            );
            assert_eq!(
                (
                    store.render_settings.fps,
                    store.render_settings.widget_update_rate
                ),
                (24.0, 2)
            );
            assert!(store.render_dialog.is_open(), "the window shows the render");
        });
        // A second start while busy fails.
        assert!(matches!(
            run(&dispatcher, &fakes, "render_start", json!({})),
            ToolOutcome::Failed(_)
        ));
        let summary = run(&dispatcher, &fakes, "ovrley_summary", json!({}));
        let ToolOutcome::Ok(summary) = summary else {
            panic!()
        };
        assert_eq!(summary["render"]["busy"], true);
        // Wait: the reply comes when the render ends.
        let (now, receiver) = start(&dispatcher, "render_progress", json!({"waitSeconds": 60}));
        assert!(now.is_none());
        let id = started["renderId"].as_u64().unwrap();
        fakes.render.mailbox.emit_progress(&RenderProgress {
            render_id: id,
            current: 10,
            total: 10,
            status: RenderStatus::Complete,
            filename: Some("overlay_1.mov".into()),
            ..RenderProgress::default()
        });
        let ToolOutcome::Ok(progress) = settle(&dispatcher, &fakes, &receiver) else {
            panic!()
        };
        assert_eq!(
            (
                progress["status"].clone(),
                progress["finished"].clone(),
                progress["percent"].clone()
            ),
            (json!("complete"), json!(true), json!(100.0))
        );
    }

    #[test]
    fn an_existing_output_asks_the_user_and_the_call_waits_for_the_answer() {
        let (dispatcher, fakes) = dispatcher();
        fakes
            .render
            .single_results
            .lock()
            .unwrap()
            .push_back(Err(CoreError::OutputExists("exists".into())));
        let (now, receiver) = start(&dispatcher, "render_start", json!({}));
        assert!(now.is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !dispatcher.with_store(|store| store.render_dialog.pending_overwrite.is_some()) {
            fakes.ui_post.drain(&dispatcher);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(receiver.try_recv().is_err(), "the call waits for the user");
        dispatcher.dispatch(|store, _| ovrley_app::features::render_video::cancel_overwrite(store));
        assert_eq!(
            receiver.recv().unwrap(),
            ToolOutcome::Ok(json!({"started": false}))
        );

        // Answered: the overwrite renders.
        dispatcher
            .dispatch(|store, _| ovrley_app::features::render_video::close_render_dialog(store));
        fakes
            .render
            .single_results
            .lock()
            .unwrap()
            .push_back(Err(CoreError::OutputExists("exists".into())));
        let (_, receiver) = start(&dispatcher, "render_start", json!({}));
        while !dispatcher.with_store(|store| store.render_dialog.pending_overwrite.is_some()) {
            fakes.ui_post.drain(&dispatcher);
            std::thread::sleep(Duration::from_millis(2));
        }
        dispatcher.dispatch(ovrley_app::features::render_video::confirm_overwrite);
        let ToolOutcome::Ok(started) = settle(&dispatcher, &fakes, &receiver) else {
            panic!()
        };
        assert_eq!(started["started"], true);
        assert!(fakes.render.calls().iter().any(|call| matches!(
            call,
            FakeRenderCall::SubmitSingle {
                overwrite: true,
                ..
            }
        )));
    }

    #[test]
    fn the_question_dropped_with_the_window_cancels_the_call() {
        let (dispatcher, fakes) = dispatcher();
        fakes
            .render
            .single_results
            .lock()
            .unwrap()
            .push_back(Err(CoreError::OutputExists("exists".into())));
        let (_, receiver) = start(&dispatcher, "render_start", json!({}));
        while !dispatcher.with_store(|store| store.render_dialog.pending_overwrite.is_some()) {
            fakes.ui_post.drain(&dispatcher);
            std::thread::sleep(Duration::from_millis(2));
        }
        dispatcher
            .dispatch(|store, _| ovrley_app::features::render_video::close_render_dialog(store));
        assert!(
            matches!(receiver.recv().unwrap(), ToolOutcome::Failed(message) if message == ovrley_app::store::agent::CANCELLED)
        );
    }

    #[test]
    fn progress_answers_at_the_deadline_and_cancel_stops_the_render() {
        let (dispatcher, fakes) = dispatcher();
        assert!(
            matches!(
                run(&dispatcher, &fakes, "render_cancel", json!({})),
                ToolOutcome::Failed(_)
            ),
            "nothing runs"
        );
        let ToolOutcome::Ok(_) = run(&dispatcher, &fakes, "render_start", json!({})) else {
            panic!()
        };
        let started = Instant::now();
        let ToolOutcome::Ok(progress) = run(
            &dispatcher,
            &fakes,
            "render_progress",
            json!({"waitSeconds": 0.05}),
        ) else {
            panic!()
        };
        assert!(started.elapsed() >= Duration::from_millis(50));
        assert_eq!(progress["busy"], true);
        assert_eq!(
            run(&dispatcher, &fakes, "render_cancel", json!({})),
            ToolOutcome::Ok(json!({"cancelling": true}))
        );
        assert!(fakes.render.calls().contains(&FakeRenderCall::Cancel));
        assert!(matches!(
            run(
                &dispatcher,
                &fakes,
                "render_progress",
                json!({"waitSeconds": 601})
            ),
            ToolOutcome::InvalidParams(_)
        ));
    }

    #[test]
    fn a_start_fails_with_the_reason_start_would_be_disabled() {
        let (dispatcher, fakes) = dispatcher();
        assert_eq!(
            run(
                &dispatcher,
                &fakes,
                "render_start",
                json!({"exportMode": "composite"})
            ),
            ToolOutcome::Failed("Full video export needs an imported video.".into())
        );
        assert!(
            !dispatcher.with_store(|store| store.render_dialog.is_open()),
            "a failed start closes the dialog"
        );
        assert!(
            matches!(run(&dispatcher, &fakes, "render_start", json!({"widgetUpdateRate": 7})), ToolOutcome::Failed(message) if message.contains("widget update rate"))
        );
        assert!(
            matches!(run(&dispatcher, &fakes, "render_start", json!({"outputPath": "relative.mov"})), ToolOutcome::Failed(message) if message.contains("absolute"))
        );
        dispatcher.dispatch(|store, _| store.media.clear_activity_file(true));
        assert_eq!(
            run(&dispatcher, &fakes, "render_start", json!({})),
            ToolOutcome::Failed("Load a GPX/FIT activity first".into())
        );
    }
}

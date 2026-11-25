//! Rendering (`RenderVideo`): the header's render actions, the render dialog's settings, progress
//! and queue, and the overwrite and missing-video notices.

use crate::projection::{models_equal, replace_if_changed, set_if_changed, strings};
use crate::{AppWindow, Choice, ChoiceKind, RenderProgressView, RenderQueueRow, RenderVideo};
use ovrley_app::features::render_video::codecs::{Acceleration, OutputFormat};
use ovrley_app::features::render_video::view::{
    render_dialog_view, render_notices_view, ProgressView, SelectEntry,
};
use ovrley_app::features::render_video::{self as render, DialogPhase};
use ovrley_app::store::{Dispatcher, Store};
use ovrley_core::encode::quality::QualityType;
use ovrley_core::encode::render_settings::{ExportMode, OutputMode};
use slint::{ComponentHandle, SharedString};

fn choice(entry: &SelectEntry) -> Choice {
    Choice {
        kind: entry.kind.map_or(ChoiceKind::Item, ChoiceKind::from),
        value: entry.value.as_str().into(),
        label: entry.label.as_str().into(),
        disabled: entry.disabled,
        trailing: entry.trailing.as_str().into(),
        help: entry.help.as_str().into(),
        ..Default::default()
    }
}

fn progress_view(view: &ProgressView) -> RenderProgressView {
    RenderProgressView {
        finished: view.finished,
        title: view.title.as_str().into(),
        subtitle: view.subtitle.as_str().into(),
        summary: strings(&view.summary),
        percent_text: view.percent_text.as_str().into(),
        frames_text: view.frames_text.as_str().into(),
        percent: view.percent,
        show_stats: view.show_stats,
        fps: view.fps.as_str().into(),
        remaining: view.remaining.as_str().into(),
        cancel_shown: view.cancel_shown,
        cancelling: view.cancelling,
    }
}

/// Whether the shown progress equals `next`, its summary compared by rows.
fn same_progress(current: &RenderProgressView, next: &RenderProgressView) -> bool {
    *current == RenderProgressView { summary: current.summary.clone(), ..next.clone() } && models_equal(&current.summary, &next.summary)
}

pub fn project(window: &AppWindow, store: &Store) {
    let global = window.global::<RenderVideo>();
    let header = render::header_actions_view(store);
    set_if_changed(global.get_header_start_disabled(), header.start_disabled, |value| global.set_header_start_disabled(value));
    set_if_changed(global.get_header_start_tooltip(), SharedString::from(header.start_tooltip.as_str()), |value| global.set_header_start_tooltip(value));
    set_if_changed(global.get_rendering(), header.rendering, |value| global.set_rendering(value));
    set_if_changed(global.get_png_shown(), header.png_shown, |value| global.set_png_shown(value));
    let notices = render_notices_view(store);
    set_if_changed(
        global.get_overwrite_open(),
        notices.overwrite_path.is_some(),
        |value| global.set_overwrite_open(value),
    );
    if let Some(path) = &notices.overwrite_path {
        set_if_changed(
            global.get_overwrite_path(),
            SharedString::from(path.as_str()),
            |value| global.set_overwrite_path(value),
        );
    }
    set_if_changed(
        global.get_missing_open(),
        notices.missing.is_some(),
        |value| global.set_missing_open(value),
    );
    if let Some((title, description)) = &notices.missing {
        set_if_changed(
            global.get_missing_title(),
            SharedString::from(title.as_str()),
            |value| global.set_missing_title(value),
        );
        set_if_changed(
            global.get_missing_description(),
            SharedString::from(description.as_str()),
            |value| global.set_missing_description(value),
        );
    }

    let Some(view) = render_dialog_view(store) else {
        set_if_changed(global.get_open(), false, |value| global.set_open(value));
        return;
    };
    let text = |value: &str| SharedString::from(value);
    set_if_changed(
        global.get_progress_phase(),
        view.phase == DialogPhase::Progress,
        |value| global.set_progress_phase(value),
    );
    set_if_changed(global.get_dismissable(), view.dismissable, |value| {
        global.set_dismissable(value)
    });
    set_if_changed(global.get_wide(), view.wide, |value| global.set_wide(value));
    set_if_changed(global.get_has_videos(), view.has_videos, |value| {
        global.set_has_videos(value)
    });
    set_if_changed(
        global.get_mismatch(),
        text(view.mismatch.as_deref().unwrap_or_default()),
        |value| global.set_mismatch(value),
    );

    let settings = &view.settings;
    set_if_changed(
        global.get_show_export_mode(),
        settings.show_export_mode,
        |value| global.set_show_export_mode(value),
    );
    set_if_changed(
        global.get_composite(),
        settings.export_mode == ExportMode::Composite,
        |value| global.set_composite(value),
    );
    set_if_changed(
        global.get_show_output_modes(),
        settings.show_output_modes,
        |value| global.set_show_output_modes(value),
    );
    set_if_changed(
        global.get_separate(),
        settings.output_mode == OutputMode::Separate,
        |value| global.set_separate(value),
    );
    set_if_changed(global.get_locked(), settings.locked, |value| {
        global.set_locked(value)
    });
    set_if_changed(
        global.get_fps_locked_text(),
        text(settings.fps_locked_text.as_deref().unwrap_or_default()),
        |value| global.set_fps_locked_text(value),
    );
    set_if_changed(global.get_fps_mode(), text(&settings.fps_mode), |value| {
        global.set_fps_mode(value)
    });
    set_if_changed(global.get_fps(), text(&settings.fps), |value| {
        global.set_fps(value)
    });
    let rates = settings
        .update_rates
        .iter()
        .map(|rate| Choice {
            kind: ChoiceKind::Item,
            value: rate.to_string().into(),
            label: format!("1/{rate}").into(),
            disabled: settings.locked,
            ..Default::default()
        })
        .collect();
    replace_if_changed(global.get_update_rates(), rates, |model| {
        global.set_update_rates(model)
    });
    set_if_changed(
        global.get_update_rate(),
        SharedString::from(settings.update_rate.to_string()),
        |value| global.set_update_rate(value),
    );
    set_if_changed(
        global.get_container_fps_text(),
        text(settings.container_fps_text.as_deref().unwrap_or_default()),
        |value| global.set_container_fps_text(value),
    );
    replace_if_changed(
        global.get_formats(),
        settings.formats.iter().map(choice).collect(),
        |model| global.set_formats(model),
    );
    set_if_changed(global.get_format(), text(&settings.format), |value| {
        global.set_format(value)
    });
    replace_if_changed(
        global.get_accelerations(),
        settings.accelerations.iter().map(choice).collect(),
        |model| global.set_accelerations(model),
    );
    set_if_changed(
        global.get_acceleration(),
        text(&settings.acceleration),
        |value| global.set_acceleration(value),
    );
    set_if_changed(
        global.get_show_quality(),
        settings.quality.is_some(),
        |value| global.set_show_quality(value),
    );
    if let Some(quality) = &settings.quality {
        set_if_changed(global.get_bitrate(), quality.bitrate, |value| {
            global.set_bitrate(value)
        });
        set_if_changed(
            global.get_quality_label(),
            text(&quality.value_label),
            |value| global.set_quality_label(value),
        );
        set_if_changed(
            global.get_slider_value(),
            quality.slider_value as f32,
            |value| global.set_slider_value(value),
        );
        set_if_changed(
            global.get_slider_minimum(),
            quality.minimum as f32,
            |value| global.set_slider_minimum(value),
        );
        set_if_changed(
            global.get_slider_maximum(),
            quality.maximum as f32,
            |value| global.set_slider_maximum(value),
        );
        set_if_changed(global.get_slider_step(), quality.step as f32, |value| {
            global.set_slider_step(value)
        });
    }
    set_if_changed(
        global.get_show_full_activity(),
        settings.show_render_full_activity,
        |value| global.set_show_full_activity(value),
    );
    set_if_changed(
        global.get_full_activity(),
        settings.render_full_activity,
        |value| global.set_full_activity(value),
    );
    set_if_changed(
        global.get_show_folder(),
        settings.folder_text.is_some(),
        |value| global.set_show_folder(value),
    );
    set_if_changed(
        global.get_folder_text(),
        text(settings.folder_text.as_deref().unwrap_or_default()),
        |value| global.set_folder_text(value),
    );
    set_if_changed(
        global.get_output_path(),
        text(&settings.output_path),
        |value| global.set_output_path(value),
    );
    set_if_changed(
        global.get_path_error(),
        text(settings.path_error.as_deref().unwrap_or_default()),
        |value| global.set_path_error(value),
    );
    set_if_changed(global.get_show_footer(), settings.show_footer, |value| {
        global.set_show_footer(value)
    });
    set_if_changed(
        global.get_cancel_disabled(),
        settings.cancel_disabled,
        |value| global.set_cancel_disabled(value),
    );
    set_if_changed(
        global.get_settings_start_disabled(),
        settings.start_disabled,
        |value| global.set_settings_start_disabled(value),
    );
    set_if_changed(
        global.get_browse_disabled(),
        settings.browse_disabled,
        |value| global.set_browse_disabled(value),
    );

    let progress = progress_view(&view.progress);
    if !same_progress(&global.get_progress(), &progress) {
        global.set_progress(progress);
    }
    set_if_changed(
        global.get_show_batch_progress(),
        view.show_batch_progress,
        |value| global.set_show_batch_progress(value),
    );
    let rows = view
        .rows
        .iter()
        .map(|row| RenderQueueRow {
            filename: row.filename.as_str().into(),
            tooltip: row.tooltip.as_str().into(),
            done: row.done,
            status_text: row.status_text.as_str().into(),
            show_progress: row.show_progress,
            progress: row.progress,
            loading: row.loading,
            has_stats: row.stats.is_some(),
            stats_fps: row
                .stats
                .as_ref()
                .map_or("", |(fps, _)| fps.as_str())
                .into(),
            stats_remaining: row
                .stats
                .as_ref()
                .map_or("", |(_, remaining)| remaining.as_str())
                .into(),
            note: row.note.as_str().into(),
            error: row.error.as_str().into(),
        })
        .collect();
    replace_if_changed(global.get_rows(), rows, |model| global.set_rows(model));
    replace_if_changed(
        global.get_blockers(),
        view.blockers.iter().map(SharedString::from).collect(),
        |model| global.set_blockers(model),
    );
    set_if_changed(
        global.get_review_error(),
        text(view.review_error.as_deref().unwrap_or_default()),
        |value| global.set_review_error(value),
    );
    set_if_changed(global.get_close_text(), text(&view.close_text), |value| {
        global.set_close_text(value)
    });
    set_if_changed(global.get_close_disabled(), view.close_disabled, |value| {
        global.set_close_disabled(value)
    });
    set_if_changed(global.get_show_start(), view.show_start, |value| {
        global.set_show_start(value)
    });
    set_if_changed(global.get_start_running(), view.start_running, |value| {
        global.set_start_running(value)
    });
    set_if_changed(global.get_start_disabled(), view.start_disabled, |value| {
        global.set_start_disabled(value)
    });
    // Opened last, so the dialog's first frame has its values.
    set_if_changed(global.get_open(), true, |value| global.set_open(value));
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<RenderVideo>();
    let d = dispatcher.handle();
    global.on_open_dialog(move || d.dispatch(render::open_render_dialog));
    let d = dispatcher.handle();
    global.on_open_output_folder(move || d.dispatch(ovrley_app::features::app_shell::open_output_directory));
    let d = dispatcher.handle();
    global.on_render_png(move || d.dispatch(render::render_preview_frame));
    let d = dispatcher.handle();
    global.on_close(move || d.dispatch(|store, _| render::close_render_dialog(store)));
    let d = dispatcher.handle();
    global.on_dismiss_missing(move || d.dispatch(|store, _| render::dismiss_missing_videos(store)));
    let d = dispatcher.handle();
    global.on_set_composite(move |composite| {
        d.dispatch(move |store, _| {
            render::set_export_mode(
                store,
                if composite {
                    ExportMode::Composite
                } else {
                    ExportMode::Transparent
                },
            )
        })
    });
    let d = dispatcher.handle();
    global.on_set_separate(move |separate| {
        d.dispatch(move |store, _| {
            render::set_output_mode(
                store,
                if separate {
                    OutputMode::Separate
                } else {
                    OutputMode::Combined
                },
            )
        })
    });
    let d = dispatcher.handle();
    global.on_select_fps_mode(move |mode| {
        d.dispatch(move |store, _| render::select_fps_mode(store, &mode))
    });
    let d = dispatcher.handle();
    global.on_set_custom_fps(move |fps| {
        // The number field commits a number; an empty or odd entry is not one.
        if let Ok(fps) = fps.trim().parse::<f64>() {
            d.dispatch(move |store, _| render::set_custom_fps(store, fps));
        }
    });
    let d = dispatcher.handle();
    global.on_set_update_rate(move |rate| {
        if let Ok(rate) = rate.parse::<u32>() {
            d.dispatch(move |store, _| render::set_update_rate(store, rate));
        }
    });
    let d = dispatcher.handle();
    global.on_select_format(move |format| {
        if let Some(format) = OutputFormat::from_value(&format) {
            d.dispatch(move |store, _| render::select_format(store, format));
        }
    });
    let d = dispatcher.handle();
    global.on_select_acceleration(move |acceleration| {
        if let Some(acceleration) = Acceleration::from_value(&acceleration) {
            d.dispatch(move |store, _| render::select_acceleration(store, acceleration));
        }
    });
    let d = dispatcher.handle();
    global.on_set_bitrate(move |bitrate| {
        d.dispatch(move |store, _| {
            render::set_quality_type(
                store,
                if bitrate {
                    QualityType::Bitrate
                } else {
                    QualityType::Quality
                },
            )
        })
    });
    let d = dispatcher.handle();
    global.on_set_quality(move |position| {
        d.dispatch(move |store, _| render::set_quality_slider(store, f64::from(position)))
    });
    let d = dispatcher.handle();
    global.on_set_full_activity(move |enabled| {
        d.dispatch(move |store, _| render::set_render_full_activity(store, enabled))
    });
    let d = dispatcher.handle();
    global.on_commit_output_path(move |path| {
        d.dispatch(move |store, _| render::set_output_path(store, path.to_string()))
    });
    let d = dispatcher.handle();
    global.on_browse(move || d.dispatch(render::browse_output_path));
    let d = dispatcher.handle();
    global.on_pick_folder(move || d.dispatch(render::pick_output_folder));
    let d = dispatcher.handle();
    global.on_start(move || {
        d.dispatch(|store, services| render::start_render(store, services, false, None))
    });
    let d = dispatcher.handle();
    global.on_cancel_render(move || d.dispatch(render::cancel_dialog_render));
    let d = dispatcher.handle();
    global.on_confirm_overwrite(move || d.dispatch(render::confirm_overwrite));
    let d = dispatcher.handle();
    global.on_cancel_overwrite(move || d.dispatch(|store, _| render::cancel_overwrite(store)));
}

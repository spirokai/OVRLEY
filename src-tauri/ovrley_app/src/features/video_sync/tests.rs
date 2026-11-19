//! Sync Doctor's workspace tests, and the parity check against React's detection and matching.

use super::detection::{
    create_sync_input, detect_activity_events, DetectionSettings, LocationEvent, TurnDirection,
};
use super::matching::{
    match_candidates, Candidate, CandidateVariant, LocationClassification, MatchScope,
};
use ovrley_core::media::manual_video_sync::Landmark;
use serde_json::Value;

fn close(left: f64, right: &Value, what: &str) {
    let right = right.as_f64().unwrap_or_else(|| panic!("{what}: {right}"));
    assert!(
        (left - right).abs() <= 1e-9 * right.abs().max(1.0),
        "{what}: rust {left} react {right}"
    );
}

fn compare_candidates(rust: &[Candidate], react: &Value, what: &str) {
    let react = react.as_array().unwrap();
    assert_eq!(rust.len(), react.len(), "{what}: candidate count");
    for (candidate, expected) in rust.iter().zip(react) {
        close(
            candidate.offset,
            &expected["offset"],
            &format!("{what} offset"),
        );
        assert_eq!(
            candidate.match_score,
            expected["matchScore"].as_f64(),
            "{what} score"
        );
        assert_eq!(
            candidate.matched_count as u64,
            expected["matchedCount"].as_u64().unwrap(),
            "{what} matched"
        );
        assert_eq!(
            candidate.eligible_count as u64,
            expected["eligibleCount"].as_u64().unwrap(),
            "{what} eligible"
        );
        let variant = match candidate.variant {
            CandidateVariant::Ordinary => "ordinary",
            CandidateVariant::LocationConflict => "locationConflict",
            CandidateVariant::LocationOnly => "locationOnly",
        };
        assert_eq!(variant, expected["variant"], "{what} variant");
        if let Some(classification) = expected.get("locationClassification") {
            let ours = match candidate.location_classification {
                LocationClassification::None => "none",
                LocationClassification::Agrees => "agrees",
                LocationClassification::Conflict => "conflict",
            };
            assert_eq!(ours, classification, "{what} classification");
        }
        let evidence = expected["evidence"].as_array().unwrap();
        assert_eq!(candidate.evidence.len(), evidence.len(), "{what} evidence");
        for (ours, theirs) in candidate.evidence.iter().zip(evidence) {
            assert_eq!(
                ours.landmark_id, theirs["landmarkId"],
                "{what} evidence landmark"
            );
            assert_eq!(ours.event_id, theirs["eventId"], "{what} evidence event");
            close(
                ours.residual_seconds,
                &theirs["residualSeconds"],
                &format!("{what} residual"),
            );
            close(ours.quality, &theirs["quality"], &format!("{what} quality"));
        }
    }
}

/// Step 0, item 5. `parity.json` holds what React's utils computed (a scratch Vitest run, method in
/// the plan's findings) on two committed fixtures: a 1 Hz ride and a track session, at two
/// sensitivities, with landmarks placed from React's events.
#[test]
fn detection_and_matching_give_reacts_numbers_on_real_activities() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("parity.json")).unwrap();
    let paths = ovrley_core::paths::AppPaths::from_repo_root(std::path::PathBuf::from("../.."));
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let path = format!(
            "{}/../ovrley_core/tests/fixtures/activity/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let activity = ovrley_core::activity::import_activity_path(&paths, &path)
            .unwrap()
            .parsed_activity;
        let input = create_sync_input(&activity);
        let settings = DetectionSettings {
            speed_threshold_kmh: case["settings"]["speedThresholdKmh"].as_f64().unwrap(),
            turn_threshold_degrees: case["settings"]["turnThresholdDegrees"].as_f64().unwrap(),
        };
        let what = format!("{name} {settings:?}");
        let location = LocationEvent {
            time: case["location"].as_f64().unwrap(),
        };
        let detection = detect_activity_events(&input, settings, Some(location));
        assert_eq!(
            input.segments.len() as u64,
            case["segments"].as_u64().unwrap(),
            "{what} segments"
        );

        let stops = case["stops"].as_array().unwrap();
        assert_eq!(detection.stops.len(), stops.len(), "{what} stops");
        for (stop, expected) in detection.stops.iter().zip(stops) {
            assert_eq!(stop.id, expected["id"]);
            close(stop.time, &expected["time"], &what);
            close(
                stop.low_speed_interval.start,
                &expected["lowSpeedInterval"]["start"],
                &what,
            );
            close(
                stop.low_speed_interval.end,
                &expected["lowSpeedInterval"]["end"],
                &what,
            );
        }
        let turns = case["turns"].as_array().unwrap();
        assert_eq!(detection.turns.len(), turns.len(), "{what} turns");
        for (turn, expected) in detection.turns.iter().zip(turns) {
            assert_eq!(turn.id, expected["id"]);
            assert_eq!(
                turn.direction == TurnDirection::Right,
                expected["type"] == "rightTurn"
            );
            close(turn.start, &expected["start"], &what);
            close(turn.end, &expected["end"], &what);
            close(turn.signed_change, &expected["signedChange"], &what);
        }
        for (series, key) in [
            (&detection.graph_series.speed, "speed"),
            (&detection.graph_series.turning, "turning"),
        ] {
            let values: Vec<f64> = series.iter().filter_map(|point| point.value).collect();
            assert_eq!(
                values.len() as u64,
                case[key]["count"].as_u64().unwrap(),
                "{what} {key} count"
            );
            close(
                values.iter().sum(),
                &case[key]["sum"],
                &format!("{what} {key} sum"),
            );
        }

        let landmarks: Vec<Landmark> = serde_json::from_value(case["landmarks"].clone()).unwrap();
        compare_candidates(
            &match_candidates(&landmarks, &detection, MatchScope::All).unwrap(),
            &case["all"],
            &format!("{what} all"),
        );
        compare_candidates(
            &match_candidates(&landmarks, &detection, MatchScope::Location).unwrap(),
            &case["loc"],
            &format!("{what} location"),
        );
    }
}

// --- The calculation (`useVideoSyncCalculation.test.jsx`) -------------------------------------

use crate::lib::video_program::fixtures::simple;
use crate::store::services::fakes::{fake_services, Fakes};
use crate::store::slices::manual_video_sync::{self as slice, CandidateStatus};
use crate::store::slices::media::{ActivitySummary, ImportedActivity};
use crate::store::slices::video_import::tests::store_with;
use crate::store::{Dispatcher, Store};
use ovrley_core::activity::schema::ParsedActivity;
use ovrley_core::media::manual_video_sync::{LandmarkKind, ManualVideoSync};
use std::sync::Arc;

fn activity(speed: &[Option<f64>]) -> ParsedActivity {
    ParsedActivity {
        sample_elapsed_seconds: (0..speed.len()).map(|index| index as f64).collect(),
        speed: speed.to_vec(),
        heading: speed.iter().map(|_| Some(0.0)).collect(),
        sample_course_points: speed.iter().map(|_| (Some(50.0), Some(14.0))).collect(),
        ..Default::default()
    }
}

fn moving() -> ParsedActivity {
    activity(&[Some(3.0); 5])
}

/// A store with the activity, the clips at their offsets and two stop landmarks on `video-1`.
fn dispatcher(
    activity: ParsedActivity,
    videos: Vec<crate::lib::video_program::ImportedVideo>,
    offsets: &[(&str, f64)],
) -> (Dispatcher, Fakes) {
    let mut store: Store = store_with(videos, offsets);
    store.media.activity_summary = Some(ActivitySummary {
        duration_seconds: 100.0,
        ..ActivitySummary::of(&activity)
    });
    store.media.parsed_activity = Some(ImportedActivity {
        activity: Arc::new(activity),
        preview_bytes: Arc::new(Vec::new()),
    });
    store.manual_video_sync = ManualVideoSync {
        video_id: Some("video-1".into()),
        landmarks: vec![
            Landmark::new("video-stop-1".into(), LandmarkKind::Stop, 4.0),
            Landmark::new("video-stop-2".into(), LandmarkKind::Stop, 12.0),
        ],
        turn_threshold_degrees: 90.0,
        ..ManualVideoSync::default()
    };
    let (parts, fakes) = fake_services(serde_json::json!({}));
    let dispatcher = Dispatcher::new(store, parts);
    dispatcher.dispatch(|_, _| {});
    (dispatcher, fakes)
}

fn one_video() -> Vec<crate::lib::video_program::ImportedVideo> {
    vec![simple("video-1", 30.0, 0)]
}

/// Waits for the workers' results and dispatches them, until no search runs.
fn settle(dispatcher: &Dispatcher, fakes: &Fakes) {
    for _ in 0..400 {
        fakes.ui_post.drain(dispatcher);
        if !dispatcher.with_store(|store| store.manual_video_sync_results.is_calculating()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the search never finished");
}

fn calculate(dispatcher: &Dispatcher, scope: MatchScope) -> Result<(), String> {
    dispatcher
        .dispatch_with_result(move |store, services| super::calculate(store, services, scope, None))
        .unwrap()
}

#[test]
fn keeps_the_location_when_detection_refreshes() {
    let (dispatcher, _) = dispatcher(moving(), one_video(), &[]);
    dispatcher.dispatch(|store, _| slice::set_detected_location(store, 2.0).unwrap());
    dispatcher.with_store(|store| {
        let detection = store.manual_video_sync_detection.as_ref().unwrap();
        assert_eq!(detection.location, Some(LocationEvent { time: 2.0 }));
        assert_eq!(detection.graph_series.speed.len(), 5, "the detection ran");
    });
}

#[test]
fn gates_the_first_search_on_usable_landmark_telemetry() {
    let (dispatcher, _) = dispatcher(activity(&[None; 5]), one_video(), &[]);
    assert!(dispatcher.with_store(|store| store.manual_video_sync_detection.is_some()));
    assert!(!dispatcher.with_store(|store| super::calculation_eligibility(store).can_calculate_all));
    assert!(calculate(&dispatcher, MatchScope::All).is_err());
    dispatcher.with_store(|store| {
        assert_eq!(
            (
                store.manual_video_sync_results.all.has_searched,
                store.manual_video_sync_results.all.status
            ),
            (false, CandidateStatus::Idle)
        )
    });
}

#[test]
fn reruns_the_search_after_a_committed_sensitivity_change() {
    let (dispatcher, fakes) = dispatcher(moving(), one_video(), &[]);
    calculate(&dispatcher, MatchScope::All).unwrap();
    settle(&dispatcher, &fakes);
    let revision = dispatcher.with_store(|store| {
        assert!(store.manual_video_sync_results.all.has_searched);
        store.manual_video_sync_results.all.revision
    });
    dispatcher.dispatch(|store, _| slice::set_speed_threshold(store, 6.0).unwrap());
    assert!(
        dispatcher.with_store(|store| store.manual_video_sync_results.is_calculating()),
        "the commit started the rerun"
    );
    settle(&dispatcher, &fakes);
    dispatcher.with_store(|store| {
        let all = &store.manual_video_sync_results.all;
        assert_eq!(
            (all.revision, all.status, all.has_searched),
            (revision + 1, CandidateStatus::Fresh, true)
        );
    });
}

#[test]
fn drops_a_result_when_a_landmark_changes_while_it_runs() {
    let (dispatcher, fakes) = dispatcher(moving(), one_video(), &[]);
    calculate(&dispatcher, MatchScope::All).unwrap();
    settle(&dispatcher, &fakes);
    let previous =
        dispatcher.with_store(|store| store.manual_video_sync_detection.clone().unwrap());
    calculate(&dispatcher, MatchScope::All).unwrap();
    dispatcher.dispatch(|store, _| slice::move_landmark(store, "video-stop-2", 13.0).unwrap());
    for _ in 0..400 {
        if !fakes.ui_post.posted.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    fakes.ui_post.drain(&dispatcher);
    dispatcher.with_store(|store| {
        assert_eq!(
            store.manual_video_sync_results.all.status,
            CandidateStatus::Stale
        );
        assert!(
            Arc::ptr_eq(
                store.manual_video_sync_detection.as_ref().unwrap(),
                &previous
            ),
            "the dropped result left the detection alone"
        );
    });
}

#[test]
fn stores_and_applies_a_location_only_result() {
    let videos = vec![simple("video-1", 30.0, 0), simple("video-2", 30.0, 1)];
    let (dispatcher, fakes) = dispatcher(moving(), videos, &[("video-1", 0.0), ("video-2", 35.0)]);
    dispatcher.dispatch(|store, _| {
        store.manual_video_sync.landmarks = vec![Landmark::new(
            "video-location".into(),
            LandmarkKind::Location,
            4.0,
        )];
        slice::set_detected_location(store, 12.0).unwrap();
    });
    calculate(&dispatcher, MatchScope::Location).unwrap();
    settle(&dispatcher, &fakes);
    let candidate = dispatcher
        .with_store(|store| store.manual_video_sync_results.location.candidates[0].clone());
    assert_eq!(
        (
            candidate.variant,
            candidate.offset,
            candidate.matched_count,
            candidate.eligible_count
        ),
        (CandidateVariant::LocationOnly, 8.0, 1, 1)
    );
    dispatcher.dispatch(move |store, _| {
        slice::apply_candidate(store, MatchScope::Location, candidate.offset).unwrap()
    });
    dispatcher.with_store(|store| {
        assert_eq!(
            store.video_import.video_edits["video-1"].offset_seconds,
            8.0
        );
        assert_eq!(
            store.video_import.video_edits["video-2"].offset_seconds,
            43.0
        );
        assert_eq!(
            store.history.past.len(),
            1,
            "applying a candidate is one undo step"
        );
    });
}

#[test]
fn reports_no_video_until_a_target_clip_is_selected() {
    let (dispatcher, _) = dispatcher(moving(), one_video(), &[]);
    dispatcher.dispatch(|store, _| store.manual_video_sync.video_id = None);
    let eligibility = dispatcher.with_store(super::calculation_eligibility);
    assert_eq!(
        (
            eligibility.can_calculate_all,
            eligibility.can_calculate_location
        ),
        (false, false)
    );
    assert_eq!(
        eligibility.all_explanation.as_deref(),
        Some("A video is required for Landmark Sync")
    );
}

// --- The timeline (`useVideoSyncTimeline.test.jsx`) -------------------------------------------

use crate::features::player::viewport::TimelineMetrics;
use crate::store::slices::editor::TimelineViewport;
use crate::store::slices::layout::LeftDrawerTool;

/// A 20 s activity, a 20 s video at offset 2 with a stop at its third second, Sync Doctor open, the
/// timeline 400 px wide showing 0 to `view_end`.
fn timeline_store(landmark: Landmark, view_end: f64) -> Dispatcher {
    let (dispatcher, _) = dispatcher(
        moving(),
        vec![simple("video-1", 20.0, 0)],
        &[("video-1", 2.0)],
    );
    dispatcher.dispatch(move |store, _| {
        store
            .media
            .activity_summary
            .as_mut()
            .unwrap()
            .duration_seconds = 20.0;
        store.manual_video_sync.landmarks = vec![landmark];
        store
            .layout
            .select_left_drawer_tool(LeftDrawerTool::VideoSync);
        store.player.viewport.metrics = TimelineMetrics {
            width_px: 400.0,
            ..TimelineMetrics::default()
        };
    });
    dispatcher.dispatch(move |store, _| {
        store.editor.timeline_viewport = TimelineViewport {
            view_start: 0.0,
            view_end,
        }
    });
    dispatcher
}

fn stop_at(second: f64) -> Landmark {
    Landmark::new("mark-1".into(), LandmarkKind::Stop, second)
}

#[test]
fn builds_both_graph_paths() {
    let detection = super::detection::Detection {
        graph_series: super::detection::GraphSeries {
            speed: vec![
                super::detection::GraphPoint {
                    time: 0.0,
                    value: Some(1.0),
                },
                super::detection::GraphPoint {
                    time: 10.0,
                    value: Some(2.0),
                },
            ],
            turning: vec![
                super::detection::GraphPoint {
                    time: 0.0,
                    value: Some(0.0),
                },
                super::detection::GraphPoint {
                    time: 10.0,
                    value: Some(30.0),
                },
            ],
        },
        ..super::detection::Detection::default()
    };
    let graph = super::timeline::graph_view(
        Some(&detection),
        TimelineViewport {
            view_start: 0.0,
            view_end: 20.0,
        },
        400.0,
        crate::i18n::Locale::En,
    );
    assert!(!graph.speed_path.is_empty() && !graph.turning_path.is_empty());
}

#[test]
fn a_handle_drag_commits_one_video_local_move_and_keeps_the_offset() {
    let dispatcher = timeline_store(stop_at(3.0), 20.0);
    let left = dispatcher.with_store(|store| super::timeline::landmark_models(store)[0].left);
    assert_eq!(left, 100.0, "timeline second 5");
    dispatcher.dispatch(|store, _| super::timeline::landmark_down(store, "mark-1"));
    dispatcher.dispatch(|store, _| super::timeline::landmark_move(store, 200.0));
    dispatcher.with_store(|store| {
        assert_eq!(
            store.manual_video_sync.landmarks[0].video_second(),
            3.0,
            "the move is a preview"
        )
    });
    dispatcher.dispatch(|store, _| super::timeline::landmark_up(store, true).unwrap());
    dispatcher.with_store(|store| {
        assert_eq!(store.manual_video_sync.landmarks[0].video_second(), 8.0);
        assert_eq!(
            store.video_import.video_edits["video-1"].offset_seconds,
            2.0
        );
    });
}

#[test]
fn a_press_without_a_move_scrubs_to_the_landmark() {
    let dispatcher = timeline_store(stop_at(3.0), 20.0);
    dispatcher.dispatch(|store, _| super::timeline::landmark_down(store, "mark-1"));
    dispatcher.dispatch(|store, _| super::timeline::landmark_up(store, true).unwrap());
    dispatcher.with_store(|store| assert_eq!(store.editor.selected_second, 5.0));
}

#[test]
fn the_displayed_offset_moves_the_handles_without_committing() {
    let dispatcher = timeline_store(stop_at(3.0), 20.0);
    dispatcher.dispatch(|store, _| {
        store.video_import.video_drag_preview =
            Some(crate::lib::video_program::collection::VideoDragPreview {
                video_id: None,
                delta_seconds: 3.0,
            });
    });
    dispatcher.with_store(|store| {
        assert_eq!(
            super::timeline::landmark_models(store)[0].left,
            160.0,
            "timeline second 8"
        );
        assert_eq!(store.manual_video_sync.landmarks[0].video_second(), 3.0);
    });
}

#[test]
fn off_view_landmarks_clamp_to_an_inert_edge_marker() {
    let dispatcher = timeline_store(
        Landmark::new("late".into(), LandmarkKind::RightTurn, 15.0),
        10.0,
    );
    let model = dispatcher.with_store(|store| super::timeline::landmark_models(store)[0].clone());
    assert_eq!(
        (model.clamped, model.interactive, model.left),
        (true, false, 400.0)
    );
}

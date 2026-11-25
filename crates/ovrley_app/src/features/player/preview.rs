//! The video preview: what it shows at the playhead and which shot waits next, and the rendered
//! blends of the transitions. The players live in `ovrley_ui`, and the video clock in
//! `features::player::playback`.
//!
//! A shot is a stretch one player plays from one file: a piece, or a transition's rendered blend in
//! the last seconds of the outgoing piece. Two players take turns at the cuts between shots.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use ovrley_core::encode::ffmpeg::transition_proxy::{TransitionProxyRequest, TransitionProxySource};
use ovrley_core::media::video_edits::{TransitionKind, VideoResolution};
use crate::features::player::{playback, video_sync_mode};
use crate::features::shell::EditorBackgroundMode;
use crate::store::Store;
use crate::store::services::Services;
use crate::video::collection::TimelineProgram;
use crate::video::program::{resolve_preview_target, resolve_program_position, Join, Piece, PreviewTarget, Program, ProgramPosition};

/// What one player plays. Times are seconds of the shot's file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot<'a> {
    pub piece: &'a Piece,
    pub path: &'a str,
    /// The playhead's second.
    pub second: f64,
    /// Where the next shot takes over.
    pub end: f64,
    /// What turns a second of the file into one of the piece's source; not 0 only for a blend.
    pub source_shift: f64,
    /// Inside a piece: the shot can be the clock.
    pub inside: bool,
}

/// Where the waiting player parks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue<'a> {
    pub path: &'a str,
    pub second: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placeholder {
    Missing,
    Failed,
    NoGpu,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Preview<'a> {
    pub shot: Option<Shot<'a>>,
    pub next: Option<Cue<'a>>,
    pub placeholder: Option<Placeholder>,
    /// Outside the pieces the frame shows at 20%.
    pub dimmed: bool,
    pub excluded: bool,
    pub overlap: bool,
    pub rendering_transition: bool,
}

/// Where the surface shows: the canvas in the video background mode, and Sync Doctor's screen.
pub fn surface_visible(store: &Store) -> bool {
    video_sync_mode(store)
        || store.editor_shell.editor_background_mode == EditorBackgroundMode::Video
}

/// A piece's file, or why there is none to play.
fn file_of<'a>(store: &'a Store, piece: &Piece) -> Result<&'a str, Placeholder> {
    let video = store
        .video_import
        .imported_videos
        .iter()
        .find(|video| video.id == piece.video_id)
        .expect("a piece's video is imported");
    if video.missing {
        Err(Placeholder::Missing)
    } else if store.player.unplayable_paths.contains(&video.path) {
        Err(Placeholder::Failed)
    } else {
        Ok(&video.path)
    }
}

/// The two pieces are one stretch of the file, as at a plain split: playback runs on without a seek.
fn runs_on(piece: &Piece, next: &Piece, join: &Join) -> bool {
    let half_frame = piece.fps.frames_to_seconds(1) / 2.0;
    piece.video_id == next.video_id
        && join.transition.is_hard_cut()
        && join.gap_seconds < half_frame
        && (next.source_start_second - piece.source_end_second).abs() < half_frame
}

/// The first shot after `piece` that needs the other player.
fn next_cue<'a>(store: &'a Store, program: &'a Program, piece: &Piece) -> Option<Cue<'a>> {
    let index = program
        .pieces
        .iter()
        .position(|candidate| candidate.id == piece.id)?;
    for (offset, join) in program.joins[index..].iter().enumerate() {
        let (from, to) = (
            &program.pieces[index + offset],
            &program.pieces[index + offset + 1],
        );
        if let Some(path) = store.transition_proxies.ready_path(join) {
            return Some(Cue { path, second: 0.0 });
        }
        if !runs_on(from, to, join) {
            return file_of(store, to).ok().map(|path| Cue {
                path,
                second: to.source_start_second,
            });
        }
    }
    None
}

/// The preview at the playhead, on the displayed program, so that a dragged clip
/// shows the frame it would put under the playhead.
pub fn resolve_preview(store: &Store) -> Preview<'_> {
    if !store.player.video_clock_available {
        return Preview {
            placeholder: Some(Placeholder::NoGpu),
            ..Preview::default()
        };
    }
    let program = &store.player.timeline.timeline().displayed.program;
    let second = store.editor.selected_second;
    let Some(PreviewTarget {
        piece,
        source_second,
        is_inside,
        is_excluded,
        ..
    }) = resolve_preview_target(program, second)
    else {
        return Preview::default();
    };
    let path = match file_of(store, piece) {
        Ok(path) => path,
        Err(placeholder) => {
            return Preview {
                placeholder: Some(placeholder),
                ..Preview::default()
            }
        }
    };
    let mut preview = Preview {
        dimmed: !is_inside,
        excluded: is_excluded,
        overlap: program
            .overlaps
            .iter()
            .any(|overlap| second >= overlap.start_second && second < overlap.end_second),
        ..Preview::default()
    };
    let mut shot = Shot {
        piece,
        path,
        second: source_second,
        end: piece.source_end_second,
        source_shift: 0.0,
        inside: is_inside,
    };
    // A dragged clip previews its own frame, without transitions.
    if store.video_import.video_drag_preview.is_some() {
        preview.shot = Some(shot);
        return preview;
    }
    let proxies = &store.transition_proxies;
    if !is_excluded {
        preview.next = next_cue(store, program, piece);
    }
    if is_inside {
        match resolve_program_position(program, second) {
            ProgramPosition::Transition {
                next_piece,
                join,
                incoming_source_second,
                ..
            } => match proxies.ready_path(join) {
                Some(proxy) => {
                    let duration = join.transition.duration_seconds();
                    shot = Shot {
                        path: proxy,
                        second: incoming_source_second - next_piece.source_start_second,
                        end: duration,
                        source_shift: piece.source_end_second - duration,
                        ..shot
                    };
                    // The blend played the incoming piece's first seconds.
                    preview.next = file_of(store, next_piece).ok().map(|path| Cue {
                        path,
                        second: next_piece.source_start_second + duration,
                    });
                }
                None => preview.rendering_transition = proxies.is_pending(join),
            },
            ProgramPosition::Piece {
                join: Some(join), ..
            } if proxies.ready_path(join).is_some() => {
                shot.end -= join.transition.duration_seconds();
            }
            _ => {}
        }
    }
    preview.shot = Some(shot);
    preview
}

/// MDK could not play `path`: a blend falls back to the hard cut, and a video's pieces play on the
/// timeline clock, as a missing video's do.
pub fn playback_failed(store: &mut Store, path: String, now: Instant) {
    if !store.transition_proxies.fail(&path) {
        store.player.unplayable_paths.insert(path);
    }
    playback::resume_from_playhead(store, now);
}

// --- Transition blends ------------------------------------------------------------------------
//
// A short FFmpeg render of each transition's blend, which the preview plays as one more shot
// between the two pieces. Session state: not undoable and not saved.

/// Proxies are at most this tall, and never taller than the outgoing video.
const PROXY_MAX_HEIGHT: u32 = 360;

#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    /// Everything the blend depends on.
    pub key: String,
    pub join_key: String,
    pub window_start_second: f64,
    pub request: TransitionProxyRequest,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Proxy {
    Rendering,
    Ready {
        path: String,
    },
    /// Not retried until its join changes; the preview cuts hard.
    Failed,
}

#[derive(Debug, Default)]
pub struct TransitionProxies {
    /// The committed timeline the jobs were listed from.
    timeline: Option<Rc<TimelineProgram>>,
    jobs: Vec<Job>,
    proxies: BTreeMap<String, Proxy>,
}

impl TransitionProxies {
    fn state_of(&self, join: &Join) -> Option<&Proxy> {
        let job = self.jobs.iter().find(|job| job.join_key == join.key)?;
        self.proxies.get(&job.key)
    }

    /// The rendered blend of a join. Without one the preview cuts hard.
    pub fn ready_path(&self, join: &Join) -> Option<&str> {
        match self.state_of(join)? {
            Proxy::Ready { path } => Some(path),
            _ => None,
        }
    }

    /// A join's blend is still to come.
    pub fn is_pending(&self, join: &Join) -> bool {
        self.jobs.iter().any(|job| job.join_key == join.key)
            && matches!(self.state_of(join), None | Some(Proxy::Rendering))
    }

    /// A rendered blend at `path` that a join still needs.
    pub fn is_ready(&self, path: &str) -> bool {
        self.proxies
            .values()
            .any(|proxy| matches!(proxy, Proxy::Ready { path: ready } if ready == path))
    }

    /// Marks the proxy at `path` failed. Whether `path` was a proxy's.
    pub fn fail(&mut self, path: &str) -> bool {
        let found = self
            .proxies
            .values_mut()
            .find(|proxy| matches!(proxy, Proxy::Ready { path: ready } if ready == path));
        found.map(|proxy| *proxy = Proxy::Failed).is_some()
    }
}

fn proxy_size(resolution: VideoResolution) -> (u32, u32) {
    let even = |value: f64| (2.0 * (value / 2.0).round()).max(2.0) as u32;
    let height = even(f64::from(PROXY_MAX_HEIGHT.min(resolution.height)));
    (
        even(f64::from(height) * f64::from(resolution.width) / f64::from(resolution.height)),
        height,
    )
}

/// One per `xfade` join of two videos whose files are present.
fn list_jobs(timeline: &TimelineProgram) -> Vec<Job> {
    let program = &timeline.program;
    let clip = |id: &str| {
        timeline
            .clips
            .iter()
            .find(|clip| clip.id() == id)
            .expect("pieces reference clips of their timeline")
    };
    program
        .joins
        .iter()
        .filter_map(|join| {
            let TransitionKind::Xfade(name) = join.transition.kind() else {
                return None;
            };
            let (from, to) = (
                program.piece(&join.from_piece_id),
                program.piece(&join.to_piece_id),
            );
            if from.missing || to.missing {
                return None;
            }
            let (outgoing, incoming) = (&clip(&from.video_id).video, &clip(&to.video_id).video);
            let duration_seconds = join.transition.duration_seconds();
            let (width, height) = proxy_size(outgoing.resolution);
            let (fps_num, fps_den) = outgoing.fps.components();
            let request = TransitionProxyRequest {
                outgoing: TransitionProxySource {
                    path: outgoing.path.clone(),
                    start_second: from.source_end_second - duration_seconds,
                    has_audio: outgoing.has_audio,
                },
                incoming: TransitionProxySource {
                    path: incoming.path.clone(),
                    start_second: to.source_start_second,
                    has_audio: incoming.has_audio,
                },
                transition: name.to_string(),
                duration_seconds,
                width,
                height,
                fps_num,
                fps_den,
            };
            Some(Job {
                key: format!("{request:?}"),
                join_key: join.key.clone(),
                window_start_second: from.timeline_end_second - duration_seconds,
                request,
            })
        })
        .collect()
}

/// The missing proxy whose window is nearest the playhead.
fn next_job<'a>(
    jobs: &'a [Job],
    proxies: &BTreeMap<String, Proxy>,
    second: f64,
) -> Option<&'a Job> {
    let distance = |job: &Job| {
        (job.window_start_second - second)
            .max(second - (job.window_start_second + job.request.duration_seconds))
            .max(0.0)
    };
    jobs.iter()
        .filter(|job| !proxies.contains_key(&job.key))
        .min_by(|left, right| distance(left).total_cmp(&distance(right)))
}

fn release(services: &Services, path: String) {
    let tools = Arc::clone(&services.video_tools);
    services.workers.spawn("transition-proxy-release", move || {
        if let Err(error) = tools.remove_transition_proxy(&path) {
            log::error!("could not delete the transition preview {path}: {error}");
        }
        Box::new(|_: &mut Store, _: &mut Services| {})
    });
}

/// The reconcile step: releases proxies no join needs any more and renders one missing proxy at a
/// time. Without players nothing shows proxies, so none render.
pub fn reconcile_transition_proxies(store: &mut Store, services: &mut Services) {
    if !store.player.video_clock_available {
        return;
    }
    let committed = &store.player.timeline.timeline().committed;
    let state = &mut store.transition_proxies;
    if !state
        .timeline
        .as_ref()
        .is_some_and(|timeline| Rc::ptr_eq(timeline, committed))
    {
        state.timeline = Some(Rc::clone(committed));
        state.jobs = list_jobs(committed);
        let jobs = &state.jobs;
        state.proxies.retain(|key, proxy| {
            if *proxy == Proxy::Rendering || jobs.iter().any(|job| job.key == *key) {
                return true;
            }
            if let Proxy::Ready { path } = proxy {
                release(services, path.clone());
            }
            false
        });
    }
    if state
        .proxies
        .values()
        .any(|proxy| *proxy == Proxy::Rendering)
    {
        return;
    }
    let Some(job) = next_job(&state.jobs, &state.proxies, store.editor.selected_second).cloned()
    else {
        return;
    };
    state.proxies.insert(job.key.clone(), Proxy::Rendering);
    let tools = Arc::clone(&services.video_tools);
    services.workers.spawn("transition-proxy", move || {
        let result = tools.render_transition_proxy(&job.request);
        Box::new(move |store: &mut Store, services: &mut Services| {
            settle(store, services, job.key, result);
        })
    });
}

fn settle(store: &mut Store, services: &Services, key: String, result: Result<String, String>) {
    let state = &mut store.transition_proxies;
    let wanted = state.jobs.iter().any(|job| job.key == key);
    match result {
        Ok(path) if !wanted => {
            state.proxies.remove(&key);
            release(services, path);
        }
        Ok(path) => {
            state.proxies.insert(key, Proxy::Ready { path });
        }
        Err(error) => {
            log::error!("transition preview failed: {error}");
            if wanted {
                state.proxies.insert(key, Proxy::Failed);
            } else {
                state.proxies.remove(&key);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::features::player::reconcile_timeline;
    use crate::video::fixtures::{cut, cut_edits, excluded, simple};
    use crate::store::videos::tests::store_with;
    use ovrley_core::media::video_edits::SegmentEdits;
    use std::rc::Rc;
    use crate::video::collection::build_timeline_program;
    use crate::video::fixtures::edits_at;
    use crate::video::ImportedVideo;
    use ovrley_core::encode::ffmpeg::transitions::TransitionDefaults;

    pub(crate) fn fade() -> TransitionDefaults {
        TransitionDefaults {
            transition_type: "fade".into(),
            duration_seconds: 2.0,
            split_gap_threshold_seconds: 2.0,
        }
    }

    /// Every join's blend, rendered: its path is `/proxies/<n>.mp4`.
    pub(crate) fn render_all(store: &mut Store) {
        let committed = Rc::clone(&store.player.timeline.timeline().committed);
        let state = &mut store.transition_proxies;
        state.jobs = list_jobs(&committed);
        state.proxies = state
            .jobs
            .iter()
            .enumerate()
            .map(|(index, job)| {
                (
                    job.key.clone(),
                    Proxy::Ready {
                        path: format!("/proxies/{index}.mp4"),
                    },
                )
            })
            .collect();
    }

    #[test]
    fn jobs_blend_the_end_of_a_piece_into_the_start_of_the_next() {
        let videos = [
            ImportedVideo {
                resolution: VideoResolution {
                    width: 3840,
                    height: 2160,
                },
                ..simple("ride", 60.0, 0)
            },
            simple("climb", 30.0, 1),
        ];
        let edits = edits_at(&videos, &[("climb", 100.0)]);
        let jobs = list_jobs(&build_timeline_program(&videos, &edits, None, &fade()));
        let [job] = jobs.as_slice() else {
            panic!("one job, got {jobs:?}")
        };
        assert_eq!(
            (job.window_start_second, job.request.duration_seconds),
            (58.0, 2.0)
        );
        assert_eq!(
            (
                job.request.outgoing.start_second,
                job.request.incoming.start_second
            ),
            (58.0, 0.0)
        );
        assert_eq!(
            (
                job.request.width,
                job.request.height,
                job.request.transition.as_str()
            ),
            (640, 360, "fade")
        );

        let cuts = TransitionDefaults {
            transition_type: "none".into(),
            ..fade()
        };
        assert!(list_jobs(&build_timeline_program(&videos, &edits, None, &cuts)).is_empty());
        let missing = [
            videos[0].clone(),
            ImportedVideo {
                missing: true,
                ..videos[1].clone()
            },
        ];
        assert!(list_jobs(&build_timeline_program(&missing, &edits, None, &fade())).is_empty());
    }

    #[test]
    fn the_missing_proxy_nearest_the_playhead_renders_first() {
        // A path per video: joins with the same content share one proxy.
        let videos = [("a", 0), ("b", 1), ("c", 2)].map(|(id, order)| ImportedVideo {
            path: format!("/videos/{id}.mp4"),
            ..simple(id, 10.0, order)
        });
        let jobs = list_jobs(&build_timeline_program(
            &videos,
            &edits_at(&videos, &[("b", 20.0), ("c", 100.0)]),
            None,
            &fade(),
        ));
        let (near, far) = (&jobs[0], &jobs[1]);
        let mut proxies = BTreeMap::new();
        assert_eq!(next_job(&jobs, &proxies, 0.0), Some(near));
        assert_eq!(next_job(&jobs, &proxies, 28.0), Some(far));
        proxies.insert(far.key.clone(), Proxy::Failed);
        assert_eq!(
            next_job(&jobs, &proxies, 28.0),
            Some(near),
            "a failure is not retried"
        );
    }

    fn store(videos: Vec<ImportedVideo>, offsets: &[(&str, f64)]) -> Store {
        let mut store = store_with(videos, offsets);
        store.player.video_clock_available = true;
        reconcile_timeline(&mut store);
        store
    }

    fn at(store: &mut Store, second: f64) -> Preview<'_> {
        store.editor.selected_second = second;
        resolve_preview(store)
    }

    #[test]
    fn outside_the_pieces_the_neighbouring_frame_shows_dimmed() {
        let ride = simple("ride", 10.0, 0);
        let climb = ImportedVideo {
            path: "/videos/climb.mp4".into(),
            ..simple("climb", 10.0, 1)
        };
        let mut store = store(
            vec![ride.clone(), climb.clone()],
            &[("ride", 5.0), ("climb", 30.0)],
        );
        for (second, path, frame, next) in [
            (0.0, &ride.path, 0.0, Some(&climb.path)),
            (20.0, &climb.path, 0.0, None),
            (50.0, &climb.path, 299.0 / 30.0, None),
        ] {
            let preview = at(&mut store, second);
            let shot = preview.shot.unwrap();
            assert_eq!(
                (shot.path, shot.second, shot.inside, preview.dimmed),
                (path.as_str(), frame, false, true),
                "at {second}"
            );
            assert_eq!(
                preview.next,
                next.map(|path| Cue { path, second: 0.0 }),
                "at {second}"
            );
        }
        let playing = at(&mut store, 7.0);
        assert_eq!((playing.shot.unwrap().second, playing.dimmed), (2.0, false));
        assert_eq!(
            playing.next,
            Some(Cue {
                path: &climb.path,
                second: 0.0
            })
        );
    }

    #[test]
    fn the_player_waits_past_plain_splits_and_at_the_next_real_cut() {
        let mut store = store(vec![simple("ride", 10.0, 0)], &[]);
        let edits = cut_edits(
            &[
                cut("split", 60, SegmentEdits::INCLUDED),
                cut("out", 120, excluded()),
                cut("in", 180, SegmentEdits::INCLUDED),
            ],
            SegmentEdits::INCLUDED,
        );
        store.video_import.video_edits = Rc::new([("ride".to_string(), edits)].into());
        reconcile_timeline(&mut store);
        assert_eq!(at(&mut store, 1.0).next.map(|cue| cue.second), Some(6.0));
        let preview = at(&mut store, 5.0);
        assert_eq!(
            (
                preview.excluded,
                preview.dimmed,
                preview.shot.unwrap().second,
                preview.next
            ),
            (true, true, 5.0, None)
        );
    }

    #[test]
    fn a_rendered_transition_is_a_shot_between_the_two_pieces() {
        let climb = ImportedVideo {
            path: "/videos/climb.mp4".into(),
            ..simple("climb", 30.0, 1)
        };
        let mut store = store(
            vec![simple("ride", 60.0, 0), climb.clone()],
            &[("climb", 100.0)],
        );
        store.video_import.transition_defaults = fade();
        reconcile_timeline(&mut store);
        assert_eq!(
            at(&mut store, 30.0).shot.unwrap().end,
            60.0,
            "a hard cut until the blend is rendered"
        );

        render_all(&mut store);
        let before = at(&mut store, 30.0);
        assert_eq!(
            (before.shot.unwrap().end, before.next),
            (
                58.0,
                Some(Cue {
                    path: "/proxies/0.mp4",
                    second: 0.0
                })
            )
        );
        let inside = at(&mut store, 59.0);
        let shot = inside.shot.unwrap();
        assert_eq!(
            (shot.path, shot.second, shot.end, shot.source_shift),
            ("/proxies/0.mp4", 1.0, 2.0, 58.0)
        );
        assert_eq!(
            inside.next,
            Some(Cue {
                path: &climb.path,
                second: 2.0
            })
        );
    }

    #[test]
    fn a_missing_or_failed_video_and_no_gpu_show_their_placeholder() {
        let mut missing = store(
            vec![ImportedVideo {
                missing: true,
                ..simple("ride", 10.0, 0)
            }],
            &[],
        );
        assert_eq!(
            at(&mut missing, 1.0).placeholder,
            Some(Placeholder::Missing)
        );
        let ride = simple("ride", 10.0, 0);
        let mut failed = store(vec![ride.clone()], &[]);
        playback_failed(&mut failed, ride.path, Instant::now());
        assert_eq!(at(&mut failed, 1.0).placeholder, Some(Placeholder::Failed));
        failed.player.video_clock_available = false;
        assert_eq!(at(&mut failed, 1.0).placeholder, Some(Placeholder::NoGpu));
    }
}

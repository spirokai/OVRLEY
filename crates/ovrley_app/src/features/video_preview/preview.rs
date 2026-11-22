//! What the video preview shows at the playhead and which shot waits next (Phase 6B): the rules of
//! `useVideoPreview` that MDK does not cover. A pure read of the store. The players live in
//! `ovrley_ui`, and the video clock in `features::player::playback`.
//!
//! A shot is a stretch one player plays from one file: a piece, or a transition's rendered blend in
//! the last seconds of the outgoing piece. Two players take turns at the cuts between shots.

use crate::features::app_shell::EditorBackgroundMode;
use crate::features::player::{playback, video_sync_mode};
use crate::lib::video_program::program::{
    resolve_preview_target, resolve_program_position, Join, Piece, PreviewTarget, Program,
    ProgramPosition,
};
use crate::store::Store;
use std::time::Instant;

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

/// `useVideoPreview`'s result at the playhead, on the displayed program, so that a dragged clip
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
                shot.end -= join.transition.duration_seconds()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::player::timeline::reconcile_timeline;
    use crate::features::video_preview::transition_proxies::tests::{fade, render_all};
    use crate::lib::video_program::fixtures::{cut, cut_edits, excluded, simple};
    use crate::lib::video_program::ImportedVideo;
    use crate::store::slices::video_import::tests::store_with;
    use ovrley_core::media::video_edits::SegmentEdits;
    use std::rc::Rc;

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

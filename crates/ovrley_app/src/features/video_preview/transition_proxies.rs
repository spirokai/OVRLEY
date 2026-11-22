//! Port of `lib/video-program/transition-proxies.js`'s jobs and of `useTransitionProxyQueue`: a short
//! FFmpeg render of each transition's blend (Phase 6B D8), which the preview plays as one more shot
//! between the two pieces.
//!
//! The window itself is `resolve_program_position`'s. Session state: not undoable and not saved.

use crate::lib::video_program::collection::TimelineProgram;
use crate::lib::video_program::program::Join;
use crate::store::services::{Services, TransitionProxyRequest, TransitionProxySource};
use crate::store::Store;
use ovrley_core::media::video_edits::{TransitionKind, VideoResolution};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

/// `PROXY_MAX_HEIGHT`: proxies are at most this tall, and never taller than the outgoing video.
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
    let even = |value: f64| (2.0 * crate::js::round(value / 2.0)).max(2.0) as u32;
    let height = even(f64::from(PROXY_MAX_HEIGHT.min(resolution.height)));
    (
        even(f64::from(height) * f64::from(resolution.width) / f64::from(resolution.height)),
        height,
    )
}

/// `listTransitionProxyJobs`: one per `xfade` join of two videos whose files are present.
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

/// `pickNextTransitionProxyJob`: the missing proxy whose window is nearest the playhead.
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
            settle(store, services, job.key, result)
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
    use crate::lib::video_program::collection::build_timeline_program;
    use crate::lib::video_program::fixtures::{edits_at, simple};
    use crate::lib::video_program::ImportedVideo;
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
}

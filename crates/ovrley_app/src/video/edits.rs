//! The edits of the imported-video collection, shared by the store and the import review's draft.
//!
//! Placement: an automatic clip follows its detected recording time (against the external
//! activity, else relative to the earliest clip) plus the shared correction of the collection; a
//! clip whose time cannot be determined sits next to its placement anchor. Each clip has its own
//! timezone reading. A clip placed by hand keeps its start when times are detected again. Shifts
//! move every clip and keep their relative placement. Cuts and segments: see `segments`.
//!
//! Each edit changes the collection in place; the store holds the collection copy-on-write. An
//! edit with arguments the UI never sends fails with an error.

use std::collections::{BTreeSet, HashMap};
use ovrley_core::media::edits::{AnchorSide, Cut, FIRST_SEGMENT_KEY, OffsetSource, PlacementAnchor, SegmentEdits, TimezoneMode, Transition, VideoEdits};
use crate::video::{new_id, ImportedVideo, VideoEditsMap};
use crate::video::collection::{join_video_edits, sort_clips_by_timeline};
use crate::video::segments::TrimSide;
use crate::video::sync::{parse_video_filename_creation_time, place_videos_by_creation_time, resolve_video_sync, Placement, RecordingTime, SyncActivity};

/// The collection an edit changes: the videos and their edits, kept in step, and the external
/// activity they are placed against (`None` without one).
pub struct CollectionDraft<'a> {
    pub videos: &'a mut Vec<ImportedVideo>,
    pub edits: &'a mut VideoEditsMap,
    pub activity: Option<&'a SyncActivity>,
}

/// Where a clip's recording time comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum CreationTimeSource {
    /// The time the probe detected.
    Detected,
    /// The `YYYYMMDD_HHMMSS` timestamp of its file name, read as local time.
    Filename,
}

impl ImportedVideo {
    /// The recording time placement reads.
    pub fn recording_time(&self) -> RecordingTime<'_> {
        RecordingTime { creation_time: self.creation_time.as_deref(), time_source: self.time_source.as_deref(), duration_seconds: self.duration_seconds }
    }
}

pub fn find_video<'a>(videos: &'a [ImportedVideo], video_id: &str) -> Result<&'a ImportedVideo, String> {
    videos.iter().find(|video| video.id == video_id).ok_or_else(|| format!("Unknown imported video: {video_id}"))
}

impl CollectionDraft<'_> {
    fn video_mut(&mut self, video_id: &str) -> Result<&mut ImportedVideo, String> {
        self.videos.iter_mut().find(|video| video.id == video_id).ok_or_else(|| format!("Unknown imported video: {video_id}"))
    }

    fn edits_mut(&mut self, video_id: &str) -> &mut VideoEdits {
        self.edits.get_mut(video_id).expect("every video has edits")
    }

    /// One clip's start, set by hand; detecting times again keeps it.
    pub fn set_video_offset(&mut self, video_id: &str, seconds: f64) -> Result<(), String> {
        require_finite(seconds, "Video offset")?;
        self.video_mut(video_id)?.sync_warning = None;
        let edits = self.edits_mut(video_id);
        edits.offset_seconds = seconds;
        edits.offset_source = OffsetSource::Manual;
        Ok(())
    }

    /// Every clip by the same amount, keeping their relative placement.
    pub fn shift_video_offsets(&mut self, delta_seconds: f64) -> Result<(), String> {
        require_finite(delta_seconds, "Video offset shift")?;
        if self.videos.is_empty() {
            return Err("A video must be loaded before shifting video offsets".into());
        }
        for video in self.videos.iter_mut() {
            video.sync_warning = None;
            self.edits.get_mut(&video.id).expect("every video has edits").offset_seconds += delta_seconds;
        }
        Ok(())
    }

    /// One clip with its edits. Clips anchored to it inherit its anchor, or keep
    /// their position when it had none.
    pub fn remove_video(&mut self, video_id: &str) -> Result<(), String> {
        find_video(self.videos, video_id)?;
        self.videos.retain(|video| video.id != video_id);
        let removed_anchor = self.edits.remove(video_id).expect("every video has edits").placement_anchor;
        for edits in self.edits.values_mut() {
            if edits.placement_anchor.as_ref().is_some_and(|anchor| anchor.video_id == video_id) {
                edits.placement_anchor = removed_anchor.clone();
            }
        }
        Ok(())
    }

    /// Splits a clip at a frame. The new segment keeps the export and overlay state of
    /// the segment it comes from and follows the transition defaults. Returns the new cut's id.
    pub fn cut_video(&mut self, video_id: &str, frame: i64) -> Result<String, String> {
        let frame_count = find_video(self.videos, video_id)?.frame_count;
        let edits = self.edits_mut(video_id);
        if frame <= 0 || frame >= frame_count {
            return Err(format!("A cut must lie inside its video: frame {frame}"));
        }
        if edits.cuts.iter().any(|cut| cut.frame == frame) {
            return Err(format!("The video is already cut at frame {frame}"));
        }
        let id = new_id();
        let position = edits.cuts.iter().take_while(|cut| cut.frame < frame).count();
        let previous_key = position.checked_sub(1).map_or(FIRST_SEGMENT_KEY.to_string(), |index| edits.cuts[index].id.clone());
        let previous = edits.segments[&previous_key];
        edits.segments.insert(id.clone(), SegmentEdits { excluded: previous.excluded, transition_in: None, skip_overlay: previous.skip_overlay });
        edits.cuts.insert(position, Cut { id: id.clone(), frame });
        Ok(id)
    }

    /// Cuts before the first cut (start) or after the last (end) and excludes the
    /// outer part. A start trim hands the clip's incoming transition to the part that now starts it.
    pub fn trim_video(&mut self, video_id: &str, side: TrimSide, frame: i64) -> Result<String, String> {
        find_video(self.videos, video_id)?;
        let edits = self.edits_mut(video_id);
        let outer = match side {
            TrimSide::Start => FIRST_SEGMENT_KEY.to_string(),
            TrimSide::End => edits.cuts.last().map_or(FIRST_SEGMENT_KEY.to_string(), |cut| cut.id.clone()),
        };
        if edits.segments[&outer].excluded {
            let name = if side == TrimSide::Start { "start" } else { "end" };
            return Err(format!("The {name} of the video is already excluded"));
        }
        let passes = match side {
            TrimSide::Start => edits.cuts.first().is_some_and(|cut| frame >= cut.frame),
            TrimSide::End => edits.cuts.last().is_some_and(|cut| frame <= cut.frame),
        };
        if passes {
            return Err(format!("A trim cannot pass a cut: frame {frame}"));
        }
        let id = self.cut_video(video_id, frame)?;
        let segments = &mut self.edits_mut(video_id).segments;
        match side {
            TrimSide::Start => {
                let first = segments[FIRST_SEGMENT_KEY];
                segments.get_mut(&id).expect("the cut has a segment").transition_in = first.transition_in;
                segments.insert(FIRST_SEGMENT_KEY.into(), SegmentEdits { excluded: true, transition_in: None, skip_overlay: first.skip_overlay });
            }
            TrimSide::End => segments.get_mut(&id).expect("the cut has a segment").excluded = true,
        }
        Ok(id)
    }

    /// A cut to another frame strictly between its neighbours (or the clip's ends).
    /// Segment edits stay keyed by the cut.
    pub fn move_cut(&mut self, video_id: &str, cut_id: &str, frame: i64) -> Result<(), String> {
        let frame_count = find_video(self.videos, video_id)?.frame_count;
        let cuts = &mut self.edits_mut(video_id).cuts;
        let index = cuts.iter().position(|cut| cut.id == cut_id).ok_or_else(|| format!("Unknown cut: {cut_id}"))?;
        let lower = if index == 0 { 0 } else { cuts[index - 1].frame };
        let upper = cuts.get(index + 1).map_or(frame_count, |cut| cut.frame);
        if frame <= lower || frame >= upper {
            return Err(format!("A cut must lie between its neighbours: frame {frame}"));
        }
        cuts[index].frame = frame;
        Ok(())
    }

    /// Merges a cut's two segments under the earlier key. The merged segment keeps
    /// the earlier segment's incoming transition (or takes back the clip's from the part after an
    /// excluded first part, where `trim_video` handed it) and takes the given state.
    pub fn remove_cut(&mut self, video_id: &str, cut_id: &str, excluded: bool, skip_overlay: bool) -> Result<(), String> {
        find_video(self.videos, video_id)?;
        let edits = self.edits_mut(video_id);
        let earlier_key = segment_before_cut(edits, cut_id)?;
        let earlier = edits.segments[&earlier_key];
        let starts_trimmed_video = earlier_key == FIRST_SEGMENT_KEY && earlier.excluded;
        let transition_in = if starts_trimmed_video { edits.segments[cut_id].transition_in } else { earlier.transition_in };
        edits.segments.insert(earlier_key, SegmentEdits { excluded, skip_overlay, transition_in });
        edits.cuts.retain(|cut| cut.id != cut_id);
        edits.segments.remove(cut_id);
        Ok(())
    }

    fn segment_mut(&mut self, video_id: &str, segment_key: &str) -> Result<&mut SegmentEdits, String> {
        find_video(self.videos, video_id)?;
        self.edits_mut(video_id).segments.get_mut(segment_key).ok_or_else(|| format!("Unknown video segment: {segment_key}"))
    }

    pub fn set_segment_excluded(&mut self, video_id: &str, segment_key: &str, excluded: bool) -> Result<(), String> {
        self.segment_mut(video_id, segment_key)?.excluded = excluded;
        Ok(())
    }

    pub fn set_segment_skip_overlay(&mut self, video_id: &str, segment_key: &str, skip_overlay: bool) -> Result<(), String> {
        self.segment_mut(video_id, segment_key)?.skip_overlay = skip_overlay;
        Ok(())
    }

    /// An override (validated by [`Transition::new`]), or `None` for the
    /// defaults.
    pub fn set_segment_transition(&mut self, video_id: &str, segment_key: &str, transition: Option<Transition>) -> Result<(), String> {
        self.segment_mut(video_id, segment_key)?.transition_in = transition;
        Ok(())
    }

    // Placement by recording time.

    /// Every clip against the activity, each in its own timezone
    /// reading, else at detected time relative to the earliest clip.
    fn detect_placements(&self) -> HashMap<String, Placement> {
        let placements: Vec<Placement> = match self.activity {
            None => place_videos_by_creation_time(&self.videos.iter().map(ImportedVideo::recording_time).collect::<Vec<_>>()),
            Some(activity) => self.videos.iter().map(|video| resolve_video_sync(&video.recording_time(), activity, self.edits[&video.id].timezone_mode)).collect(),
        };
        self.videos.iter().map(|video| video.id.clone()).zip(placements).collect()
    }

    /// Moves a clip to its placement, which also settles its timezone reading.
    fn apply_placement(&mut self, video_id: &str, placement: &Placement, offset_seconds: f64) {
        self.video_mut(video_id).expect("placed clips exist").sync_warning = placement.sync_warning;
        let edits = self.edits_mut(video_id);
        edits.offset_seconds = offset_seconds;
        if placement.determined {
            edits.timezone_mode = placement.timezone_mode;
        }
    }

    /// The clips whose recording time cannot be determined.
    pub fn find_undetermined_video_ids(&self) -> BTreeSet<String> {
        let placements = self.detect_placements();
        self.videos.iter().filter(|video| !placements[&video.id].determined).map(|video| video.id.clone()).collect()
    }

    /// Directly before or after the anchor clip.
    fn place_at_anchor(&mut self, video_id: &str) {
        let anchor = self.edits[video_id].placement_anchor.clone().expect("an anchored clip");
        let anchor_offset = self.edits[&anchor.video_id].offset_seconds;
        let duration = |id: &str| find_video(self.videos, id).expect("anchors name clips").duration_seconds;
        let offset = match anchor.side {
            AnchorSide::Before => anchor_offset - duration(video_id),
            AnchorSide::After => anchor_offset + duration(&anchor.video_id),
        };
        self.edits_mut(video_id).offset_seconds = offset;
    }

    /// Moves clips next to their anchors, then along anchor chains every clip
    /// anchored to a moved clip that `follows` admits. Returns the moved clips, each after its
    /// anchor.
    fn place_at_anchors(&mut self, video_ids: Vec<String>, follows: &dyn Fn(&Self, &str) -> bool) -> Vec<String> {
        let mut moved = video_ids;
        let mut index = 0;
        while index < moved.len() {
            let moved_id = moved[index].clone();
            self.place_at_anchor(&moved_id);
            let anchored: Vec<String> = self
                .videos
                .iter()
                .filter(|video| self.edits[&video.id].placement_anchor.as_ref().is_some_and(|anchor| anchor.video_id == moved_id) && follows(self, &video.id))
                .map(|video| video.id.clone())
                .collect();
            moved.extend(anchored);
            index += 1;
        }
        moved
    }

    fn follows_if_automatic(&self, video_id: &str) -> bool {
        self.edits[video_id].offset_source == OffsetSource::Auto
    }

    /// Places some automatic clips by detected time plus the shared
    /// correction of the others (committed minus detected offset of the earliest other automatic
    /// clip whose time is known; zero without one). Without an activity, detected times are only
    /// relative, so a clip placed by hand serves when no automatic clip does. Returns the placed
    /// clips whose time could not be determined, in input order.
    fn place_like_the_collection(&mut self, video_ids: &[String]) -> Vec<String> {
        let placements = self.detect_placements();
        let clips = join_video_edits(self.videos, self.edits);
        let references: Vec<_> = sort_clips_by_timeline(&clips)
            .into_iter()
            .filter(|clip| !video_ids.iter().any(|id| id == clip.id()) && placements[clip.id()].determined && (clip.edits.offset_source == OffsetSource::Auto || self.activity.is_none()))
            .collect();
        let reference = references.iter().find(|clip| clip.edits.offset_source == OffsetSource::Auto).or(references.first());
        let correction = reference.map_or(0.0, |clip| clip.offset_seconds() - placements[clip.id()].offset_seconds);
        for video_id in video_ids {
            let placement = placements[video_id.as_str()];
            let offset = if placement.determined { placement.offset_seconds + correction } else { placement.offset_seconds };
            self.apply_placement(video_id, &placement, offset);
        }
        video_ids.iter().filter(|id| !placements[id.as_str()].determined).cloned().collect()
    }

    /// Adds videos not yet in the collection. Existing clips keep their placement;
    /// new clips are placed like the collection, and one whose time cannot be determined goes
    /// after the last placed clip. Returns the added clips whose time could not be determined.
    pub fn add_videos(&mut self, videos: Vec<ImportedVideo>) -> Result<Vec<String>, String> {
        if videos.is_empty() {
            return Err("At least one video is required".into());
        }
        let ids: Vec<String> = videos.iter().map(|video| video.id.clone()).collect();
        for video in videos {
            self.edits.insert(video.id.clone(), VideoEdits::default());
            self.videos.push(video);
        }
        let undetermined = self.place_like_the_collection(&ids);
        let mut unplaced: BTreeSet<String> = undetermined.iter().cloned().collect();
        for video_id in &undetermined {
            unplaced.remove(video_id);
            let clips = join_video_edits(self.videos, self.edits);
            let last = sort_clips_by_timeline(&clips).into_iter().rfind(|clip| clip.id() != video_id && !unplaced.contains(clip.id())).map(|clip| clip.id().to_string());
            if let Some(last) = last {
                self.anchor_video(video_id, &last, AnchorSide::After)?;
            }
        }
        Ok(undetermined)
    }

    /// Places a clip directly before or after another clip and keeps it there for
    /// as long as its own time cannot be determined. Clips anchored to it follow.
    pub fn anchor_video(&mut self, video_id: &str, anchor_video_id: &str, side: AnchorSide) -> Result<(), String> {
        find_video(self.videos, video_id)?;
        find_video(self.videos, anchor_video_id)?;
        let mut current = Some(anchor_video_id.to_string());
        while let Some(id) = current {
            if id == video_id {
                return Err("A video cannot be placed relative to itself".into());
            }
            current = self.edits[&id].placement_anchor.as_ref().map(|anchor| anchor.video_id.clone());
        }
        let edits = self.edits_mut(video_id);
        edits.placement_anchor = Some(PlacementAnchor { video_id: anchor_video_id.into(), side });
        edits.offset_source = OffsetSource::Auto;
        self.video_mut(video_id)?.sync_warning = None;
        self.place_at_anchors(vec![video_id.into()], &Self::follows_if_automatic);
        Ok(())
    }

    /// Moves an anchored clip next to another, as if inserted into a list. A
    /// clip already anchored to that side moves along, next to the moved clip. Clips anchored to
    /// the moved clip first take over its previous anchor (or stay put without one), so the move
    /// never forms a cycle.
    pub fn move_video_next_to(&mut self, video_id: &str, anchor_video_id: &str, side: AnchorSide) -> Result<(), String> {
        let anchored_to = |draft: &Self, target: &str, target_side: Option<AnchorSide>| -> Vec<String> {
            draft
                .videos
                .iter()
                .filter(|video| {
                    video.id != video_id
                        && draft.edits[&video.id].placement_anchor.as_ref().is_some_and(|anchor| anchor.video_id == target && target_side.is_none_or(|side| anchor.side == side))
                })
                .map(|video| video.id.clone())
                .collect()
        };
        find_video(self.videos, video_id)?;
        let previous_anchor = self.edits[video_id].placement_anchor.clone();
        let followers = anchored_to(self, video_id, None);
        for follower in &followers {
            self.edits_mut(follower).placement_anchor = previous_anchor.clone();
        }
        let occupants = anchored_to(self, anchor_video_id, Some(side));
        self.anchor_video(video_id, anchor_video_id, side)?;
        for occupant in occupants {
            self.edits_mut(&occupant).placement_anchor = Some(PlacementAnchor { video_id: video_id.into(), side });
            self.place_at_anchors(vec![occupant], &Self::follows_if_automatic);
        }
        for follower in followers {
            if self.edits[&follower].placement_anchor.is_some() {
                self.place_at_anchors(vec![follower], &Self::follows_if_automatic);
            }
        }
        Ok(())
    }

    /// Every automatic clip by detected time; one whose time cannot be
    /// determined sits next to its anchor, or keeps its start without one. Clips placed by hand
    /// keep their start.
    pub fn place_automatic_videos(&mut self) -> Result<(), String> {
        if self.videos.is_empty() {
            return Ok(());
        }
        let placements = self.detect_placements();
        let mut anchored: Vec<String> = Vec::new();
        let ids: Vec<String> = self.videos.iter().map(|video| video.id.clone()).collect();
        for video_id in &ids {
            let edits = &self.edits[video_id];
            if edits.offset_source == OffsetSource::Manual {
                continue;
            }
            let placement = placements[video_id];
            if !placement.determined {
                if edits.placement_anchor.is_some() {
                    anchored.push(video_id.clone());
                }
                continue;
            }
            self.apply_placement(video_id, &placement, placement.offset_seconds);
        }
        // Anchored clips are placed from the ones anchored to placed clips (anchors never form cycles).
        let roots: Vec<String> = anchored
            .iter()
            .filter(|id| !anchored.contains(&self.edits[id.as_str()].placement_anchor.as_ref().expect("anchored").video_id))
            .cloned()
            .collect();
        let placed = self.place_at_anchors(roots, &|_, id| anchored.iter().any(|anchored| anchored == id));
        if placed.len() != anchored.len() {
            return Err("Video placement anchors must reference imported videos".into());
        }
        for video_id in placed {
            self.video_mut(&video_id)?.sync_warning = None;
        }
        Ok(())
    }

    /// A clip set by hand back to automatic placement.
    pub fn reset_video_offset(&mut self, video_id: &str) -> Result<(), String> {
        find_video(self.videos, video_id)?;
        self.edits_mut(video_id).offset_source = OffsetSource::Auto;
        self.place_video(video_id);
        Ok(())
    }

    /// One automatic clip by detected time like the collection, or next to its
    /// anchor when its time cannot be determined.
    fn place_video(&mut self, video_id: &str) {
        let undetermined = self.place_like_the_collection(&[video_id.to_string()]);
        if !undetermined.is_empty() && self.edits[video_id].placement_anchor.is_some() {
            self.video_mut(video_id).expect("placed clips exist").sync_warning = None;
            self.place_at_anchors(vec![video_id.into()], &Self::follows_if_automatic);
        }
    }

    /// A clip placed by hand keeps its start.
    fn place_video_if_automatic(&mut self, video_id: &str) {
        if self.edits[video_id].offset_source == OffsetSource::Auto {
            self.place_video(video_id);
        }
    }

    /// Every clip by its detected recording time again, also the ones set by hand.
    pub fn sync_videos(&mut self) -> Result<(), String> {
        for edits in self.edits.values_mut() {
            edits.offset_source = OffsetSource::Auto;
        }
        self.place_automatic_videos()
    }

    /// One clip's camera clock in local time or UTC, then placed again.
    pub fn set_video_timezone_mode(&mut self, video_id: &str, mode: TimezoneMode) -> Result<(), String> {
        find_video(self.videos, video_id)?;
        self.edits_mut(video_id).timezone_mode = Some(mode);
        self.place_video_if_automatic(video_id);
        Ok(())
    }

    /// Every clip's camera clock, then times detected again.
    pub fn set_videos_timezone_mode(&mut self, mode: TimezoneMode) -> Result<(), String> {
        for edits in self.edits.values_mut() {
            edits.timezone_mode = Some(mode);
        }
        self.place_automatic_videos()
    }

    /// The file name's timestamp, which is local time.
    fn apply_filename_creation_time(&mut self, video_id: &str, creation_time: String) {
        let video = self.video_mut(video_id).expect("a clip");
        video.creation_time = Some(creation_time);
        video.time_source = Some("filename".into());
        self.edits_mut(video_id).timezone_mode = Some(TimezoneMode::Local);
    }

    /// The detected time, whose timezone reading is inferred again.
    fn reset_creation_time(&mut self, video_id: &str) {
        let video = self.video_mut(video_id).expect("a clip");
        video.creation_time = video.detected_creation_time.clone();
        video.time_source = video.detected_time_source.clone();
        self.edits_mut(video_id).timezone_mode = None;
    }

    /// Where one clip's recording time comes from, then placed
    /// again.
    pub fn set_video_creation_time_source(&mut self, video_id: &str, source: CreationTimeSource) -> Result<(), String> {
        let video = find_video(self.videos, video_id)?;
        match source {
            CreationTimeSource::Filename => {
                let creation_time = parse_video_filename_creation_time(&video.path).ok_or("The video filename contains no valid YYYYMMDD_HHMMSS timestamp")?;
                self.apply_filename_creation_time(video_id, creation_time);
            }
            CreationTimeSource::Detected => self.reset_creation_time(video_id),
        }
        self.place_video_if_automatic(video_id);
        Ok(())
    }

    /// Every clip whose file name has a timestamp, then times
    /// detected again.
    pub fn apply_filename_creation_times(&mut self) -> Result<(), String> {
        let times: Vec<(String, Option<String>)> = self.videos.iter().map(|video| (video.id.clone(), parse_video_filename_creation_time(&video.path))).collect();
        if times.iter().all(|(_, time)| time.is_none()) {
            return Err("No video filename contains a valid YYYYMMDD_HHMMSS timestamp".into());
        }
        for (video_id, time) in times {
            if let Some(time) = time {
                self.apply_filename_creation_time(&video_id, time);
            }
        }
        self.place_automatic_videos()
    }

    /// Every clip back to its detected time, then times detected again.
    pub fn reset_creation_times(&mut self) -> Result<(), String> {
        let ids: Vec<String> = self.videos.iter().map(|video| video.id.clone()).collect();
        for video_id in ids {
            self.reset_creation_time(&video_id);
        }
        self.place_automatic_videos()
    }
}

fn require_finite(seconds: f64, label: &str) -> Result<(), String> {
    if seconds.is_finite() {
        Ok(())
    } else {
        Err(format!("{label} must be a finite number"))
    }
}

/// The key of the segment that ends at a cut.
pub fn segment_before_cut(edits: &VideoEdits, cut_id: &str) -> Result<String, String> {
    let index = edits.cuts.iter().position(|cut| cut.id == cut_id).ok_or_else(|| format!("Unknown cut: {cut_id}"))?;
    Ok(if index == 0 { FIRST_SEGMENT_KEY.to_string() } else { edits.cuts[index - 1].id.clone() })
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use ovrley_core::media::edits::{AnchorSide, PlacementAnchor, TransitionKind};
    use super::*;

    struct Owned {
        videos: Vec<ImportedVideo>,
        edits: VideoEditsMap,
        activity: Option<SyncActivity>,
    }

    impl Owned {
        fn ride() -> Self {
            let videos = vec![simple("ride", 10.0, 0)];
            let edits = edits_at(&videos, &[]);
            Self { videos, edits, activity: None }
        }

        fn draft(&mut self) -> CollectionDraft<'_> {
            CollectionDraft { videos: &mut self.videos, edits: &mut self.edits, activity: self.activity.as_ref() }
        }

        fn ride_edits(&self) -> &VideoEdits {
            &self.edits["ride"]
        }
    }

    fn fade(seconds: f64) -> Transition {
        Transition::new(TransitionKind::from_name("fade").unwrap(), seconds).unwrap()
    }

    #[test]
    fn a_new_segment_keeps_the_state_it_comes_from_and_follows_the_defaults() {
        let mut collection = Owned::ride();
        collection.edits.get_mut("ride").unwrap().segments.insert("start".into(), SegmentEdits { excluded: true, transition_in: Some(fade(1.0)), skip_overlay: true });
        let later = collection.draft().cut_video("ride", 200).unwrap();
        let earlier = collection.draft().cut_video("ride", 100).unwrap();
        assert_eq!(collection.ride_edits().cuts, [Cut { id: earlier.clone(), frame: 100 }, Cut { id: later.clone(), frame: 200 }]);
        let inherited = SegmentEdits { excluded: true, transition_in: None, skip_overlay: true };
        assert_eq!(collection.ride_edits().segments[&earlier], inherited);
        assert_eq!(collection.ride_edits().segments[&later], inherited);
        assert_eq!(collection.ride_edits().segments["start"].transition_in, Some(fade(1.0)));
    }

    #[test]
    fn cuts_outside_the_video_or_on_a_cut_or_an_unknown_video_fail() {
        for frame in [0, 300, 301, -1] {
            assert!(Owned::ride().draft().cut_video("ride", frame).unwrap_err().contains("must lie inside"));
        }
        let mut collection = Owned::ride();
        collection.draft().cut_video("ride", 100).unwrap();
        assert!(collection.draft().cut_video("ride", 100).unwrap_err().contains("already cut"));
        assert!(collection.draft().cut_video("missing", 100).unwrap_err().contains("Unknown imported video"));
    }

    #[test]
    fn removing_a_cut_merges_into_the_earlier_segment_which_keeps_its_transition() {
        let mut collection = Owned::ride();
        let first = collection.draft().cut_video("ride", 100).unwrap();
        let second = collection.draft().cut_video("ride", 200).unwrap();
        collection.draft().set_segment_transition("ride", &first, Some(fade(1.0))).unwrap();
        collection.draft().set_segment_excluded("ride", &second, true).unwrap();
        assert_eq!(segment_before_cut(collection.ride_edits(), &first).unwrap(), "start");
        assert_eq!(segment_before_cut(collection.ride_edits(), &second).unwrap(), first);

        collection.draft().remove_cut("ride", &second, true, false).unwrap();
        assert_eq!(collection.ride_edits().cuts, [Cut { id: first.clone(), frame: 100 }]);
        assert_eq!(collection.ride_edits().segments[&first], SegmentEdits { excluded: true, transition_in: Some(fade(1.0)), skip_overlay: false });
        assert!(!collection.ride_edits().segments.contains_key(&second));
        assert!(collection.draft().remove_cut("ride", "missing", false, false).unwrap_err().contains("Unknown cut"));
    }

    #[test]
    fn segment_edits_require_a_known_segment() {
        let mut collection = Owned::ride();
        assert!(collection.draft().set_segment_excluded("ride", "missing", true).unwrap_err().contains("Unknown video segment"));
        collection.draft().set_segment_transition("ride", "start", Some(fade(1.0))).unwrap();
        collection.draft().set_segment_transition("ride", "start", None).unwrap();
        assert_eq!(collection.ride_edits().segments["start"].transition_in, None);
        collection.draft().set_segment_skip_overlay("ride", "start", true).unwrap();
        assert!(collection.ride_edits().segments["start"].skip_overlay);
    }

    #[test]
    fn a_trim_excludes_the_outer_part_hands_on_the_transition_and_never_passes_a_cut() {
        let mut collection = Owned::ride();
        collection.draft().set_segment_transition("ride", "start", Some(fade(1.0))).unwrap();
        collection.draft().cut_video("ride", 150).unwrap();
        let trim = collection.draft().trim_video("ride", TrimSide::Start, 30).unwrap();
        assert_eq!(collection.ride_edits().cuts[0], Cut { id: trim.clone(), frame: 30 });
        assert_eq!(collection.ride_edits().segments["start"], SegmentEdits { excluded: true, transition_in: None, skip_overlay: false });
        assert_eq!(collection.ride_edits().segments[&trim], SegmentEdits { excluded: false, transition_in: Some(fade(1.0)), skip_overlay: false });
        let tail = collection.draft().trim_video("ride", TrimSide::End, 270).unwrap();
        assert_eq!(collection.ride_edits().cuts.iter().map(|cut| cut.frame).collect::<Vec<_>>(), [30, 150, 270]);
        assert!(collection.ride_edits().segments[&tail].excluded);
        assert!(collection.draft().trim_video("ride", TrimSide::Start, 20).unwrap_err().contains("already excluded"));

        let mut fresh = Owned::ride();
        fresh.draft().cut_video("ride", 150).unwrap();
        assert!(fresh.draft().trim_video("ride", TrimSide::End, 100).unwrap_err().contains("cannot pass a cut"));
    }

    #[test]
    fn removing_a_trim_cut_takes_the_transition_back_to_the_first_part() {
        let mut collection = Owned::ride();
        collection.draft().set_segment_transition("ride", "start", Some(fade(1.0))).unwrap();
        let trim = collection.draft().trim_video("ride", TrimSide::Start, 30).unwrap();
        collection.draft().remove_cut("ride", &trim, false, false).unwrap();
        assert_eq!(collection.ride_edits().segments["start"].transition_in, Some(fade(1.0)));
    }

    #[test]
    fn a_cut_moves_strictly_between_its_neighbours() {
        let mut collection = Owned::ride();
        let first = collection.draft().cut_video("ride", 100).unwrap();
        collection.draft().cut_video("ride", 200).unwrap();
        collection.draft().move_cut("ride", &first, 150).unwrap();
        assert_eq!(collection.ride_edits().cuts[0].frame, 150);
        assert!(collection.draft().move_cut("ride", &first, 200).is_err());
        assert!(collection.draft().move_cut("ride", &first, 0).is_err());
    }

    #[test]
    fn offsets_and_shifts_clear_sync_warnings() {
        let mut collection = Owned::ride();
        collection.videos[0].sync_warning = Some(crate::video::sync::SyncWarning::OutsideActivity);
        collection.draft().set_video_offset("ride", 12.0).unwrap();
        assert_eq!((collection.ride_edits().offset_seconds, collection.ride_edits().offset_source), (12.0, OffsetSource::Manual));
        assert_eq!(collection.videos[0].sync_warning, None);
        collection.draft().shift_video_offsets(-2.0).unwrap();
        assert_eq!(collection.ride_edits().offset_seconds, 10.0);
        assert!(collection.draft().set_video_offset("ride", f64::NAN).is_err());
        let mut empty = Owned { videos: Vec::new(), edits: VideoEditsMap::new(), activity: None };
        assert!(empty.draft().shift_video_offsets(1.0).is_err());
    }

    #[test]
    fn removing_a_clip_reanchors_the_clips_anchored_to_it() {
        let videos = vec![simple("a", 10.0, 0), simple("b", 10.0, 1), simple("c", 10.0, 2)];
        let mut edits = edits_at(&videos, &[]);
        edits.get_mut("b").unwrap().placement_anchor = Some(PlacementAnchor { video_id: "a".into(), side: AnchorSide::After });
        edits.get_mut("c").unwrap().placement_anchor = Some(PlacementAnchor { video_id: "b".into(), side: AnchorSide::After });
        let mut collection = Owned { videos, edits, activity: None };
        collection.draft().remove_video("b").unwrap();
        assert_eq!(collection.videos.iter().map(|video| video.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(collection.edits["c"].placement_anchor, Some(PlacementAnchor { video_id: "a".into(), side: AnchorSide::After }));
        assert!(collection.draft().remove_video("b").is_err());
    }

    // Placement.

    /// Activity 08:00:00Z–08:02:00Z, Sofia (UTC+3).
    fn sofia() -> SyncActivity {
        SyncActivity { sync_time: Some("2026-07-18T08:00:00Z".into()), end_time: Some("2026-07-18T08:02:00Z".into()), timezone: Some("Europe/Sofia".into()) }
    }

    /// Moscow (UTC+3): 10:51 clock text fits as local and as UTC.
    fn moscow() -> SyncActivity {
        SyncActivity { sync_time: Some("2026-07-18T07:20:03.000Z".into()), end_time: Some("2026-07-18T11:20:03.000Z".into()), timezone: Some("Europe/Moscow".into()) }
    }

    const CAMERA_CLOCK: (&str, &str) = ("2026-07-18T10:51:00.000000Z", "ffprobe");

    fn timed(id: &'static str, time: Option<(&str, &str)>, duration_seconds: f64, import_order: u32) -> ImportedVideo {
        let path: &'static str = Box::leak(format!("/videos/{id}.mp4").into_boxed_str());
        let mut video = video(TestVideo { id, path, duration_seconds, import_order, ..TestVideo::default() });
        if let Some((creation_time, time_source)) = time {
            video.creation_time = Some(creation_time.into());
            video.time_source = Some(time_source.into());
            video.detected_creation_time = video.creation_time.clone();
            video.detected_time_source = video.time_source.clone();
        }
        video
    }

    fn collection(activity: Option<SyncActivity>, videos: Vec<ImportedVideo>) -> Owned {
        let mut owned = Owned { videos: Vec::new(), edits: VideoEditsMap::new(), activity };
        owned.draft().add_videos(videos).unwrap();
        owned
    }

    fn offsets(owned: &Owned) -> Vec<(String, f64)> {
        owned.videos.iter().map(|video| (video.id.clone(), owned.edits[&video.id].offset_seconds)).collect()
    }

    fn expect_offsets(owned: &Owned, expected: &[(&str, f64)]) {
        let expected: Vec<(String, f64)> = expected.iter().map(|(id, offset)| (id.to_string(), *offset)).collect();
        assert_eq!(offsets(owned), expected);
    }

    #[test]
    fn without_an_activity_clips_sit_at_detected_time_relative_to_the_earliest() {
        let owned = collection(None, vec![timed("late", Some(("2026-07-18T08:05:00Z", "ffprobe")), 60.0, 0), timed("early", Some(("2026-07-18T08:00:00Z", "ffprobe")), 60.0, 1)]);
        expect_offsets(&owned, &[("late", 300.0), ("early", 0.0)]);
        assert!(owned.videos.iter().all(|video| video.sync_warning.is_none()));
    }

    #[test]
    fn with_an_activity_each_clip_syncs_and_clips_outside_keep_their_signed_offset() {
        let owned = collection(Some(sofia()), vec![timed("inside", Some(("2026-07-18T08:00:30Z", "gps")), 20.0, 0), timed("before", Some(("2026-07-18T07:58:00Z", "gps")), 20.0, 1)]);
        expect_offsets(&owned, &[("inside", 30.0), ("before", -120.0)]);
        assert_eq!(owned.videos[1].sync_warning, Some(crate::video::sync::SyncWarning::OutsideActivity));
    }

    #[test]
    fn ambiguous_clocks_infer_a_timezone_and_choosing_one_for_every_clip_places_them_again() {
        let mut owned = collection(Some(moscow()), vec![timed("video", Some(CAMERA_CLOCK), 60.0, 0)]);
        expect_offsets(&owned, &[("video", 1857.0)]);
        assert_eq!(owned.edits["video"].timezone_mode, Some(TimezoneMode::Local));
        owned.draft().set_videos_timezone_mode(TimezoneMode::Utc).unwrap();
        expect_offsets(&owned, &[("video", 12657.0)]);
        assert_eq!(owned.edits["video"].timezone_mode, Some(TimezoneMode::Utc));
    }

    #[test]
    fn one_clips_timezone_places_only_that_clip_again() {
        let mut owned = collection(Some(moscow()), vec![timed("first", Some(CAMERA_CLOCK), 60.0, 0), timed("second", Some(CAMERA_CLOCK), 60.0, 1)]);
        owned.draft().set_video_timezone_mode("second", TimezoneMode::Utc).unwrap();
        expect_offsets(&owned, &[("first", 1857.0), ("second", 12657.0)]);
        assert_eq!(owned.edits["first"].timezone_mode, Some(TimezoneMode::Local));
    }

    #[test]
    fn one_clips_time_comes_from_its_filename_as_local_time_or_from_detection() {
        let mut named = timed("named", Some(CAMERA_CLOCK), 60.0, 0);
        named.path = "/videos/GX_20260718_090000.mp4".into();
        let mut owned = collection(Some(moscow()), vec![named, timed("other", Some(CAMERA_CLOCK), 60.0, 1)]);
        owned.draft().set_video_timezone_mode("named", TimezoneMode::Utc).unwrap();
        owned.draft().set_video_creation_time_source("named", CreationTimeSource::Filename).unwrap();
        assert_eq!(owned.videos[0].time_source.as_deref(), Some("filename"));
        assert_eq!(owned.edits["named"].timezone_mode, Some(TimezoneMode::Local));
        // 09:00 Moscow time is 06:00Z, before the activity starts at 07:20:03Z.
        expect_offsets(&owned, &[("named", -4803.0), ("other", 1857.0)]);
        owned.draft().set_video_creation_time_source("named", CreationTimeSource::Detected).unwrap();
        assert_eq!((owned.videos[0].creation_time.as_deref(), owned.videos[0].time_source.as_deref()), (Some(CAMERA_CLOCK.0), Some("ffprobe")));
        expect_offsets(&owned, &[("named", 1857.0), ("other", 1857.0)]);
        let error = owned.draft().set_video_creation_time_source("other", CreationTimeSource::Filename).unwrap_err();
        assert!(error.contains("no valid YYYYMMDD_HHMMSS timestamp"));
    }

    #[test]
    fn a_clip_placed_by_hand_keeps_its_start_when_its_time_changes_until_synced() {
        let mut owned = collection(Some(moscow()), vec![timed("video", Some(CAMERA_CLOCK), 60.0, 0)]);
        owned.draft().set_video_offset("video", 100.0).unwrap();
        owned.draft().set_video_timezone_mode("video", TimezoneMode::Utc).unwrap();
        expect_offsets(&owned, &[("video", 100.0)]);
        owned.draft().reset_video_offset("video").unwrap();
        expect_offsets(&owned, &[("video", 12657.0)]);
    }

    #[test]
    fn existing_clips_keep_their_edits_and_new_clips_follow_the_shared_correction() {
        let mut owned = collection(Some(sofia()), vec![timed("first", Some(("2026-07-18T08:00:10Z", "gps")), 20.0, 0), timed("second", Some(("2026-07-18T08:00:40Z", "gps")), 20.0, 1)]);
        owned.draft().shift_video_offsets(2.0).unwrap();
        owned.draft().set_video_offset("second", 50.0).unwrap();
        owned.draft().add_videos(vec![timed("added", Some(("2026-07-18T08:01:20Z", "gps")), 20.0, 2)]).unwrap();
        expect_offsets(&owned, &[("first", 12.0), ("second", 50.0), ("added", 82.0)]);
    }

    #[test]
    fn a_clip_whose_time_is_unknown_goes_after_the_last_placed_clip() {
        let owned = collection(None, vec![timed("known", Some(("2026-07-18T08:00:00Z", "ffprobe")), 20.0, 0), timed("first", None, 20.0, 1), timed("second", None, 20.0, 2)]);
        expect_offsets(&owned, &[("known", 0.0), ("first", 20.0), ("second", 40.0)]);
        assert_eq!(owned.edits["first"].placement_anchor, Some(PlacementAnchor { video_id: "known".into(), side: AnchorSide::After }));
        assert_eq!(owned.edits["second"].placement_anchor, Some(PlacementAnchor { video_id: "first".into(), side: AnchorSide::After }));
        assert!(owned.videos.iter().all(|video| video.sync_warning.is_none()));
        let mut owned = owned;
        assert_eq!(owned.draft().find_undetermined_video_ids(), BTreeSet::from(["first".to_string(), "second".to_string()]));
    }

    fn undetermined() -> Owned {
        collection(None, vec![timed("known", Some(("2026-07-18T08:00:30Z", "gps")), 20.0, 0), timed("first", None, 20.0, 1), timed("second", None, 20.0, 2)])
    }

    #[test]
    fn anchored_clips_follow_their_anchor_until_their_time_is_known() {
        let mut owned = undetermined();
        owned.draft().anchor_video("first", "known", AnchorSide::Before).unwrap();
        expect_offsets(&owned, &[("known", 0.0), ("first", -20.0), ("second", 0.0)]);
        owned.activity = Some(sofia());
        owned.draft().place_automatic_videos().unwrap();
        expect_offsets(&owned, &[("known", 30.0), ("first", 10.0), ("second", 30.0)]);
        owned.videos[1].creation_time = Some("2026-07-18T08:01:00Z".into());
        owned.videos[1].time_source = Some("gps".into());
        owned.draft().place_automatic_videos().unwrap();
        expect_offsets(&owned, &[("known", 30.0), ("first", 60.0), ("second", 80.0)]);
    }

    #[test]
    fn moving_an_anchored_clip_past_its_follower_hands_its_anchor_on() {
        let mut owned = undetermined();
        owned.draft().move_video_next_to("first", "second", AnchorSide::After).unwrap();
        assert_eq!(owned.edits["second"].placement_anchor, Some(PlacementAnchor { video_id: "known".into(), side: AnchorSide::After }));
        assert_eq!(owned.edits["first"].placement_anchor, Some(PlacementAnchor { video_id: "second".into(), side: AnchorSide::After }));
        expect_offsets(&owned, &[("known", 0.0), ("first", 40.0), ("second", 20.0)]);
    }

    #[test]
    fn rejects_anchors_to_unknown_clips_to_itself_and_through_cycles() {
        let mut owned = undetermined();
        assert!(owned.draft().anchor_video("first", "missing", AnchorSide::After).unwrap_err().contains("Unknown imported video"));
        assert!(owned.draft().anchor_video("first", "first", AnchorSide::After).unwrap_err().contains("relative to itself"));
        assert!(owned.draft().anchor_video("first", "second", AnchorSide::Before).unwrap_err().contains("relative to itself"));
    }

    #[test]
    fn removing_a_clip_reanchors_its_followers() {
        let mut owned = undetermined();
        owned.draft().remove_video("first").unwrap();
        assert_eq!(owned.edits["second"].offset_seconds, 40.0);
        assert_eq!(owned.edits["second"].placement_anchor, Some(PlacementAnchor { video_id: "known".into(), side: AnchorSide::After }));
        owned.draft().remove_video("known").unwrap();
        assert_eq!((owned.edits["second"].offset_seconds, owned.edits["second"].placement_anchor.clone()), (40.0, None));
    }

    fn synced() -> Owned {
        collection(Some(sofia()), vec![timed("first", Some(("2026-07-18T08:00:10Z", "gps")), 20.0, 0), timed("second", Some(("2026-07-18T08:00:40Z", "gps")), 20.0, 1)])
    }

    #[test]
    fn a_start_set_by_hand_survives_detecting_times_again() {
        let mut owned = synced();
        owned.draft().set_video_offset("second", 70.0).unwrap();
        assert_eq!(owned.edits["second"].offset_source, OffsetSource::Manual);
        owned.draft().shift_video_offsets(5.0).unwrap();
        expect_offsets(&owned, &[("first", 15.0), ("second", 75.0)]);
        owned.draft().set_videos_timezone_mode(TimezoneMode::Utc).unwrap();
        expect_offsets(&owned, &[("first", 10.0), ("second", 75.0)]);
    }

    #[test]
    fn syncing_every_clip_places_the_ones_set_by_hand_again() {
        let mut owned = synced();
        owned.draft().set_video_offset("second", 70.0).unwrap();
        owned.draft().shift_video_offsets(5.0).unwrap();
        owned.draft().sync_videos().unwrap();
        expect_offsets(&owned, &[("first", 10.0), ("second", 40.0)]);
        assert_eq!(owned.edits["second"].offset_source, OffsetSource::Auto);
    }

    #[test]
    fn reset_returns_an_undeterminable_clip_next_to_its_anchor() {
        let mut owned = collection(None, vec![timed("known", Some(("2026-07-18T08:00:00Z", "ffprobe")), 20.0, 0), timed("unknown", None, 20.0, 1)]);
        owned.draft().set_video_offset("unknown", 500.0).unwrap();
        owned.draft().reset_video_offset("unknown").unwrap();
        expect_offsets(&owned, &[("known", 0.0), ("unknown", 20.0)]);
        assert_eq!(owned.videos[1].sync_warning, None);
    }

}

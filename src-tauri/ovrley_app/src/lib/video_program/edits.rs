//! Port of the player's part of `collection-edits.js`: offsets, shifts, removal, cuts, trims and
//! segment edits. Placement by recording time, anchors and timezones arrive with import in Phase 6.
//!
//! Each edit changes the collection in place; the slice holds the collection copy-on-write (D4).
//! An edit that the UI never asks for with bad arguments fails with an error, as React throws.

use super::segments::TrimSide;
use super::{new_id, ImportedVideo, VideoEditsMap};
use ovrley_core::media::video_edits::{Cut, OffsetSource, SegmentEdits, Transition, VideoEdits, FIRST_SEGMENT_KEY};

/// The collection an edit changes: the videos and their edits, kept in step.
pub struct CollectionDraft<'a> {
    pub videos: &'a mut Vec<ImportedVideo>,
    pub edits: &'a mut VideoEditsMap,
}

/// `findVideo`.
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

    /// `setVideoOffset`: one clip's start, set by hand; detecting times again keeps it.
    pub fn set_video_offset(&mut self, video_id: &str, seconds: f64) -> Result<(), String> {
        require_finite(seconds, "Video offset")?;
        self.video_mut(video_id)?.sync_warning = None;
        let edits = self.edits_mut(video_id);
        edits.offset_seconds = seconds;
        edits.offset_source = OffsetSource::Manual;
        Ok(())
    }

    /// `shiftVideoOffsets`: every clip by the same amount, keeping their relative placement.
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

    /// `removeVideo`: one clip with its edits. Clips anchored to it inherit its anchor, or keep
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

    /// `cutVideo`: splits a clip at a frame. The new segment keeps the export and overlay state of
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

    /// `trimVideo`: cuts before the first cut (start) or after the last (end) and excludes the
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

    /// `moveCut`: a cut to another frame strictly between its neighbours (or the clip's ends).
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

    /// `removeCut`: merges a cut's two segments under the earlier key. The merged segment keeps
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

    /// `setSegmentExcluded`.
    pub fn set_segment_excluded(&mut self, video_id: &str, segment_key: &str, excluded: bool) -> Result<(), String> {
        self.segment_mut(video_id, segment_key)?.excluded = excluded;
        Ok(())
    }

    /// `setSegmentSkipOverlay`.
    pub fn set_segment_skip_overlay(&mut self, video_id: &str, segment_key: &str, skip_overlay: bool) -> Result<(), String> {
        self.segment_mut(video_id, segment_key)?.skip_overlay = skip_overlay;
        Ok(())
    }

    /// `setSegmentTransition`: an override (validated by [`Transition::new`]), or `None` for the
    /// defaults.
    pub fn set_segment_transition(&mut self, video_id: &str, segment_key: &str, transition: Option<Transition>) -> Result<(), String> {
        self.segment_mut(video_id, segment_key)?.transition_in = transition;
        Ok(())
    }
}

fn require_finite(seconds: f64, label: &str) -> Result<(), String> {
    if seconds.is_finite() {
        Ok(())
    } else {
        Err(format!("{label} must be a finite number"))
    }
}

/// `segmentBeforeCut`: the key of the segment that ends at a cut.
pub fn segment_before_cut(edits: &VideoEdits, cut_id: &str) -> Result<String, String> {
    let index = edits.cuts.iter().position(|cut| cut.id == cut_id).ok_or_else(|| format!("Unknown cut: {cut_id}"))?;
    Ok(if index == 0 { FIRST_SEGMENT_KEY.to_string() } else { edits.cuts[index - 1].id.clone() })
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use ovrley_core::media::video_edits::{AnchorSide, PlacementAnchor, TransitionKind};
    use super::*;

    struct Owned {
        videos: Vec<ImportedVideo>,
        edits: VideoEditsMap,
    }

    impl Owned {
        fn ride() -> Self {
            let videos = vec![simple("ride", 10.0, 0)];
            let edits = edits_at(&videos, &[]);
            Self { videos, edits }
        }

        fn draft(&mut self) -> CollectionDraft<'_> {
            CollectionDraft { videos: &mut self.videos, edits: &mut self.edits }
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
        collection.videos[0].sync_warning = Some("far".into());
        collection.draft().set_video_offset("ride", 12.0).unwrap();
        assert_eq!((collection.ride_edits().offset_seconds, collection.ride_edits().offset_source), (12.0, OffsetSource::Manual));
        assert_eq!(collection.videos[0].sync_warning, None);
        collection.draft().shift_video_offsets(-2.0).unwrap();
        assert_eq!(collection.ride_edits().offset_seconds, 10.0);
        assert!(collection.draft().set_video_offset("ride", f64::NAN).is_err());
        let mut empty = Owned { videos: Vec::new(), edits: VideoEditsMap::new() };
        assert!(empty.draft().shift_video_offsets(1.0).is_err());
    }

    #[test]
    fn removing_a_clip_reanchors_the_clips_anchored_to_it() {
        let videos = vec![simple("a", 10.0, 0), simple("b", 10.0, 1), simple("c", 10.0, 2)];
        let mut edits = edits_at(&videos, &[]);
        edits.get_mut("b").unwrap().placement_anchor = Some(PlacementAnchor { video_id: "a".into(), side: AnchorSide::After });
        edits.get_mut("c").unwrap().placement_anchor = Some(PlacementAnchor { video_id: "b".into(), side: AnchorSide::After });
        let mut collection = Owned { videos, edits };
        collection.draft().remove_video("b").unwrap();
        assert_eq!(collection.videos.iter().map(|video| video.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(collection.edits["c"].placement_anchor, Some(PlacementAnchor { video_id: "a".into(), side: AnchorSide::After }));
        assert!(collection.draft().remove_video("b").is_err());
    }
}

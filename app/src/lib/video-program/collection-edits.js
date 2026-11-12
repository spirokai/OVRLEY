/**
 * Edits of the imported-video collection, shared by the store and the import
 * dialog's draft. Each recipe mutates an Immer draft of the collection
 * `{ importedVideos, videoEdits, parsedActivity, activitySummary }`.
 *
 * Placement: an `auto` clip follows its detected recording time (against the
 * external activity, otherwise relative to the earliest clip) plus the shared
 * correction of the collection; a clip whose time cannot be determined sits next
 * to its placement anchor. Each clip has its own Apply Timezone mode (`timezoneMode`
 * of its edits): how its camera clock is read against the activity. A `manual` clip keeps the start set by hand when times
 * are detected again. Shifts move every clip and keep their relative placement.
 *
 * Cuts and segments: see `video-segments`.
 */

import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { placeVideosByCreationTime, resolveVideoSync } from '@/lib/video-sync'
import { createVideoEdits } from './imported-video'
import { requireTransition } from './transition-catalog'
import { joinVideoEdits, sortVideosByTimeline } from './video-collection'
import { FIRST_SEGMENT_KEY } from './video-segments'

/**
 * Returns an imported video of the collection.
 * @param {{importedVideos: Array<object>}} collection Collection state.
 * @param {string} videoId Video ID.
 * @returns {object} Imported video entity.
 */
export function findVideo(collection, videoId) {
  const video = collection.importedVideos.find((candidate) => candidate.id === videoId)
  if (video === undefined) throw new Error(`Unknown imported video: ${videoId}`)
  return video
}

/** Puts a clip directly before or after its anchor clip. */
function placeAtAnchor(draft, videoId) {
  const { videoId: anchorId, side } = draft.videoEdits[videoId].placementAnchor
  const anchorOffset = draft.videoEdits[anchorId].offsetSeconds
  draft.videoEdits[videoId].offsetSeconds =
    side === 'before' ? anchorOffset - findVideo(draft, videoId).durationSeconds : anchorOffset + findVideo(draft, anchorId).durationSeconds
}

function requireFiniteOffset(seconds, label) {
  if (!Number.isFinite(seconds)) throw new Error(`${label} must be a finite number`)
}

/**
 * Detects the placement of every given clip: against the external activity when
 * one is loaded (each clip read in its own timezone mode), otherwise at detected
 * real time relative to the earliest clip.
 * @returns {Array<{offsetSeconds: number, determined: boolean, syncWarning: string|null, timezoneMode: 'utc'|'local'|null}>}
 *   Placements in input order, each with the timezone mode its clip settles on.
 */
function detectVideoPlacements(collection, videos) {
  if (collection.parsedActivity === null) return placeVideosByCreationTime(videos).map((placement) => ({ ...placement, timezoneMode: null }))
  return videos.map((video) => resolveVideoSync(video, collection.activitySummary, collection.videoEdits[video.id].timezoneMode))
}

/** Moves a clip to its detected placement, which also settles its timezone mode. */
function applyPlacement(draft, videoId, placement, offsetSeconds) {
  findVideo(draft, videoId).syncWarning = placement.syncWarning
  draft.videoEdits[videoId].offsetSeconds = offsetSeconds
  if (placement.determined) draft.videoEdits[videoId].timezoneMode = placement.timezoneMode
}

/**
 * Lists the clips whose recording time cannot be determined.
 * @param {object} collection Collection state.
 * @returns {Set<string>} Video IDs.
 */
export function findUndeterminedVideoIds(collection) {
  const placements = detectVideoPlacements(collection, collection.importedVideos)
  return new Set(collection.importedVideos.filter((_, index) => !placements[index].determined).map((video) => video.id))
}

/**
 * Moves clips next to their anchors, then along anchor chains every clip anchored
 * to a moved clip that `follows` admits (automatic clips by default).
 * @returns {Array<string>} IDs of the moved clips, each after its anchor.
 */
function placeAtAnchors(draft, videoIds, follows = (videoId) => draft.videoEdits[videoId].offsetSource === 'auto') {
  const moved = [...videoIds]
  for (const movedId of moved) {
    placeAtAnchor(draft, movedId)
    moved.push(
      ...draft.importedVideos
        .filter((video) => draft.videoEdits[video.id].placementAnchor?.videoId === movedId && follows(video.id))
        .map((video) => video.id),
    )
  }
  return moved
}

/**
 * Places some automatic clips by detected time plus the shared correction of
 * the others: committed minus detected offset of the earliest other automatic
 * clip whose time can be determined (zero without one). Without an external
 * activity detected times are only relative, so a clip placed by hand serves
 * when no automatic clip does.
 * @returns {Array<string>} IDs of the placed clips whose time could not be determined, in input order.
 */
function placeLikeTheCollection(draft, videoIds) {
  const placements = detectVideoPlacements(draft, draft.importedVideos)
  const placementsById = new Map(draft.importedVideos.map((video, index) => [video.id, placements[index]]))
  const placed = new Set(videoIds)
  const references = sortVideosByTimeline(joinVideoEdits(draft.importedVideos, draft.videoEdits)).filter(
    (clip) => !placed.has(clip.id) && placementsById.get(clip.id).determined && (clip.offsetSource === 'auto' || draft.parsedActivity === null),
  )
  const reference = references.find((clip) => clip.offsetSource === 'auto') ?? references[0]
  const correctionSeconds = reference ? reference.offsetSeconds - placementsById.get(reference.id).offsetSeconds : 0
  for (const videoId of videoIds) {
    const placement = placementsById.get(videoId)
    applyPlacement(draft, videoId, placement, placement.determined ? placement.offsetSeconds + correctionSeconds : placement.offsetSeconds)
  }
  return videoIds.filter((videoId) => !placementsById.get(videoId).determined)
}

/**
 * Adds videos not yet in the collection. Existing clips keep their placement; new
 * clips are placed like the collection (see `placeLikeTheCollection`). A new clip
 * whose time cannot be determined goes after the last placed clip.
 * @param {object} draft Collection draft.
 * @param {Array<object>} videos Imported video entities with paths not yet in the collection (the import review
 *   turns away the others).
 * @returns {Array<string>} IDs of added clips whose time could not be determined, in input order.
 */
export function addVideos(draft, videos) {
  if (videos.length === 0) throw new Error('At least one video is required')
  for (const video of videos) {
    draft.importedVideos.push(video)
    draft.videoEdits[video.id] = createVideoEdits()
  }
  const undeterminedIds = placeLikeTheCollection(
    draft,
    videos.map((video) => video.id),
  )
  const unplaced = new Set(undeterminedIds)
  for (const videoId of undeterminedIds) {
    unplaced.delete(videoId)
    const last = sortVideosByTimeline(joinVideoEdits(draft.importedVideos, draft.videoEdits))
      .filter((clip) => clip.id !== videoId && !unplaced.has(clip.id))
      .at(-1)
    if (last !== undefined) anchorVideo(draft, videoId, last.id, 'after')
  }
  return undeterminedIds
}

/**
 * Places a clip directly before or after another clip and keeps that placement
 * for as long as the clip's own time cannot be determined. Clips anchored to it follow.
 */
export function anchorVideo(draft, videoId, anchorVideoId, side) {
  findVideo(draft, videoId)
  findVideo(draft, anchorVideoId)
  if (side !== 'before' && side !== 'after') throw new Error('Placement side must be before or after')
  for (let current = anchorVideoId; current !== null; current = draft.videoEdits[current].placementAnchor?.videoId ?? null) {
    if (current === videoId) throw new Error('A video cannot be placed relative to itself')
  }
  draft.videoEdits[videoId].placementAnchor = { videoId: anchorVideoId, side }
  draft.videoEdits[videoId].offsetSource = 'auto'
  findVideo(draft, videoId).syncWarning = null
  placeAtAnchors(draft, [videoId])
}

/**
 * Moves an anchored clip next to another clip, as if inserted into a list: a clip
 * already anchored to the same side of that clip moves along, next to the moved
 * clip. Clips anchored to the moved clip first take over its previous anchor (or
 * stay where they are without one), so the move never forms an anchor cycle.
 */
export function moveVideoNextTo(draft, videoId, anchorVideoId, side) {
  const anchoredTo = (targetId, targetSide) =>
    draft.importedVideos.filter((video) => {
      const anchor = draft.videoEdits[video.id].placementAnchor
      return video.id !== videoId && anchor?.videoId === targetId && (targetSide === null || anchor.side === targetSide)
    })
  const previousAnchor = draft.videoEdits[videoId].placementAnchor
  const followers = anchoredTo(videoId, null)
  for (const follower of followers) draft.videoEdits[follower.id].placementAnchor = previousAnchor === null ? null : { ...previousAnchor }
  const occupants = anchoredTo(anchorVideoId, side)
  anchorVideo(draft, videoId, anchorVideoId, side)
  for (const occupant of occupants) {
    draft.videoEdits[occupant.id].placementAnchor = { videoId, side }
    placeAtAnchors(draft, [occupant.id])
  }
  for (const follower of followers) {
    if (draft.videoEdits[follower.id].placementAnchor !== null) placeAtAnchors(draft, [follower.id])
  }
}

/**
 * Places every automatic clip by detected time; a clip whose time cannot be
 * determined sits next to its placement anchor, or keeps its start without one.
 * Clips placed by hand keep their start.
 */
export function placeAutomaticVideos(draft) {
  if (draft.importedVideos.length === 0) return
  const placements = detectVideoPlacements(draft, draft.importedVideos)
  const anchored = new Set()
  draft.importedVideos.forEach((video, index) => {
    const edits = draft.videoEdits[video.id]
    if (edits.offsetSource === 'manual') return
    if (!placements[index].determined) {
      if (edits.placementAnchor !== null) anchored.add(video.id)
      return
    }
    applyPlacement(draft, video.id, placements[index], placements[index].offsetSeconds)
  })
  // Anchored clips are placed from the ones anchored to placed clips (anchors never form cycles).
  const roots = [...anchored].filter((videoId) => !anchored.has(draft.videoEdits[videoId].placementAnchor.videoId))
  const placed = placeAtAnchors(draft, roots, (videoId) => anchored.has(videoId))
  if (placed.length !== anchored.size) throw new Error('Video placement anchors must reference imported videos')
  for (const videoId of placed) findVideo(draft, videoId).syncWarning = null
}

/** Sets one clip's start by hand; detecting times again keeps it. */
export function setVideoOffset(draft, videoId, seconds) {
  requireFiniteOffset(seconds, 'Video offset')
  const video = findVideo(draft, videoId)
  draft.videoEdits[videoId].offsetSeconds = seconds
  draft.videoEdits[videoId].offsetSource = 'manual'
  video.syncWarning = null
}

/** Returns a clip set by hand to automatic placement: next to its anchor, or by detected time like the collection. */
export function resetVideoOffset(draft, videoId) {
  findVideo(draft, videoId)
  draft.videoEdits[videoId].offsetSource = 'auto'
  placeVideo(draft, videoId)
}

/** Places one automatic clip by detected time like the collection, or next to its anchor when its time cannot be determined. */
function placeVideo(draft, videoId) {
  const [undetermined] = placeLikeTheCollection(draft, [videoId])
  if (undetermined !== undefined && draft.videoEdits[videoId].placementAnchor !== null) {
    findVideo(draft, videoId).syncWarning = null
    placeAtAnchors(draft, [videoId])
  }
}

/** Places one clip again after its recording time changed; a clip placed by hand keeps its start. */
function placeVideoIfAutomatic(draft, videoId) {
  if (draft.videoEdits[videoId].offsetSource === 'auto') placeVideo(draft, videoId)
}

/** Places every clip by its detected recording time again, also the ones set by hand. */
export function syncVideos(draft) {
  for (const video of draft.importedVideos) draft.videoEdits[video.id].offsetSource = 'auto'
  placeAutomaticVideos(draft)
}

/** Shifts every clip by the same amount, keeping their relative placement. */
export function shiftVideoOffsets(draft, deltaSeconds) {
  requireFiniteOffset(deltaSeconds, 'Video offset shift')
  if (draft.importedVideos.length === 0) throw new Error('A video must be loaded before shifting video offsets')
  for (const video of draft.importedVideos) {
    draft.videoEdits[video.id].offsetSeconds += deltaSeconds
    video.syncWarning = null
  }
}

/** Removes one clip with its edits. Clips anchored to it inherit its anchor (or keep their position when it had none). */
export function removeVideo(draft, videoId) {
  findVideo(draft, videoId)
  const removedAnchor = draft.videoEdits[videoId].placementAnchor
  draft.importedVideos = draft.importedVideos.filter((video) => video.id !== videoId)
  delete draft.videoEdits[videoId]
  for (const edits of Object.values(draft.videoEdits)) {
    if (edits.placementAnchor?.videoId === videoId) edits.placementAnchor = removedAnchor === null ? null : { ...removedAnchor }
  }
}

/**
 * Splits a video at a frame. The new segment starts out in the same export and
 * overlay state as the segment it comes from, and follows the transition defaults.
 * @param {object} draft Collection draft.
 * @param {string} videoId Video ID.
 * @param {number} frame Integer frame strictly inside the video and not on a cut.
 * @returns {string} ID of the new cut.
 */
export function cutVideo(draft, videoId, frame) {
  const { frameCount } = findVideo(draft, videoId)
  const edits = draft.videoEdits[videoId]
  if (!Number.isInteger(frame) || frame <= 0 || frame >= frameCount) throw new Error(`A cut must lie inside its video: frame ${frame}`)
  if (edits.cuts.some((cut) => cut.frame === frame)) throw new Error(`The video is already cut at frame ${frame}`)
  const id = globalThis.crypto.randomUUID()
  const previous = edits.cuts.filter((cut) => cut.frame < frame).at(-1)
  const { excluded, skipOverlay } = edits.segments[previous?.id ?? FIRST_SEGMENT_KEY]
  edits.segments[id] = { excluded, transitionIn: null, skipOverlay }
  edits.cuts.splice(previous === undefined ? 0 : edits.cuts.indexOf(previous) + 1, 0, { id, frame })
  return id
}

/**
 * Trims the start or end of a video: cuts it at a frame before its first cut (start)
 * or after its last cut (end) and excludes the outer part. A start trim hands the
 * video's incoming transition to the part that now starts it.
 * @param {object} draft Collection draft.
 * @param {string} videoId Video ID.
 * @param {'start'|'end'} side Trimmed end of the video.
 * @param {number} frame Integer frame of the new cut.
 * @returns {string} ID of the new cut.
 */
export function trimVideo(draft, videoId, side, frame) {
  const { cuts, segments } = draft.videoEdits[videoId]
  const outer = side === 'start' ? FIRST_SEGMENT_KEY : (cuts.at(-1)?.id ?? FIRST_SEGMENT_KEY)
  if (segments[outer].excluded) throw new Error(`The ${side} of the video is already excluded`)
  const bound = side === 'start' ? cuts[0]?.frame : cuts.at(-1)?.frame
  if (bound !== undefined && (side === 'start' ? frame >= bound : frame <= bound)) throw new Error(`A trim cannot pass a cut: frame ${frame}`)
  const id = cutVideo(draft, videoId, frame)
  if (side === 'start') {
    segments[id].transitionIn = segments[FIRST_SEGMENT_KEY].transitionIn
    segments[FIRST_SEGMENT_KEY] = { excluded: true, transitionIn: null, skipOverlay: segments[FIRST_SEGMENT_KEY].skipOverlay }
  } else {
    segments[id].excluded = true
  }
  return id
}

/**
 * Returns the key of the segment that ends at a cut.
 * @param {{cuts: Array<{id: string}>}} edits Video edits.
 * @param {string} cutId Cut ID.
 * @returns {string} Segment key.
 */
export function segmentBeforeCut(edits, cutId) {
  const index = edits.cuts.findIndex((cut) => cut.id === cutId)
  if (index === -1) throw new Error(`Unknown cut: ${cutId}`)
  return index === 0 ? FIRST_SEGMENT_KEY : edits.cuts[index - 1].id
}

/**
 * Moves a cut to another frame between its neighbouring cuts. Segment edits stay
 * keyed by the cut, so both segments keep their export state and transitions.
 * @param {object} draft Collection draft.
 * @param {string} videoId Video ID.
 * @param {string} cutId Cut ID.
 * @param {number} frame Integer frame strictly between the neighbouring cuts (or the video's ends).
 */
export function moveCut(draft, videoId, cutId, frame) {
  const { frameCount } = findVideo(draft, videoId)
  const { cuts } = draft.videoEdits[videoId]
  const index = cuts.findIndex((cut) => cut.id === cutId)
  if (index === -1) throw new Error(`Unknown cut: ${cutId}`)
  const lower = index === 0 ? 0 : cuts[index - 1].frame
  const upper = cuts[index + 1]?.frame ?? frameCount
  if (!Number.isInteger(frame) || frame <= lower || frame >= upper) throw new Error(`A cut must lie between its neighbours: frame ${frame}`)
  cuts[index].frame = frame
}

/**
 * Removes a cut, merging its two segments. The merged segment keeps the earlier
 * segment's transition and takes the given export state.
 * @param {object} draft Collection draft.
 * @param {string} videoId Video ID.
 * @param {string} cutId Cut ID.
 * @param {boolean} excluded Export state of the merged segment.
 */
export function removeCut(draft, videoId, cutId, excluded) {
  findVideo(draft, videoId)
  const edits = draft.videoEdits[videoId]
  edits.segments[segmentBeforeCut(edits, cutId)].excluded = excluded
  edits.cuts = edits.cuts.filter((cut) => cut.id !== cutId)
  delete edits.segments[cutId]
}

function requireSegment(draft, videoId, segmentKey) {
  findVideo(draft, videoId)
  const segment = draft.videoEdits[videoId].segments[segmentKey]
  if (segment === undefined) throw new Error(`Unknown video segment: ${segmentKey}`)
  return segment
}

/** Excludes a segment from export or includes it again. */
export function setSegmentExcluded(draft, videoId, segmentKey, excluded) {
  requireSegment(draft, videoId, segmentKey).excluded = excluded
}

/** Hides or shows the activity widgets over a segment, in the preview and the render. */
export function setSegmentSkipOverlay(draft, videoId, segmentKey, skipOverlay) {
  if (typeof skipOverlay !== 'boolean') throw new Error('Activity overlay suppression must be boolean')
  requireSegment(draft, videoId, segmentKey).skipOverlay = skipOverlay
}

/** Overrides the transition into a segment, or with `null` returns it to the defaults. */
export function setSegmentTransition(draft, videoId, segmentKey, transition) {
  requireSegment(draft, videoId, segmentKey).transitionIn = requireTransition(transition)
}

function requireTimezoneMode(mode) {
  if (mode !== 'local' && mode !== 'utc') throw new Error('Video sync timezone mode must be local or utc')
}

/** Reads one clip's ambiguous camera clock in local time or UTC, then places it again. */
export function setVideoTimezoneMode(draft, videoId, mode) {
  findVideo(draft, videoId)
  requireTimezoneMode(mode)
  draft.videoEdits[videoId].timezoneMode = mode
  placeVideoIfAutomatic(draft, videoId)
}

/** Reads every clip's ambiguous camera clock in local time or UTC, then detects times again. */
export function setVideosTimezoneMode(draft, mode) {
  requireTimezoneMode(mode)
  for (const video of draft.importedVideos) draft.videoEdits[video.id].timezoneMode = mode
  placeAutomaticVideos(draft)
}

/** Uses a clip's `YYYYMMDD_HHMMSS` filename timestamp, which is local time. */
function applyFilenameCreationTime(draft, video, creationTime) {
  video.creationTime = creationTime
  video.timeSource = 'filename'
  draft.videoEdits[video.id].timezoneMode = 'local'
}

/** Returns a clip to its detected creation time, whose timezone mode is inferred again. */
function resetCreationTime(draft, video) {
  video.creationTime = video.detectedCreationTime
  video.timeSource = video.detectedTimeSource
  draft.videoEdits[video.id].timezoneMode = null
}

/**
 * Sets where one clip's recording time comes from, then places it again.
 * @param {object} draft Collection draft.
 * @param {string} videoId Video ID.
 * @param {'detected'|'filename'} source The detected time, or the filename timestamp.
 */
export function setVideoCreationTimeSource(draft, videoId, source) {
  const video = findVideo(draft, videoId)
  if (source === 'filename') {
    const creationTime = parseVideoFilenameCreationTime(video.path)
    if (creationTime === null) throw new Error('The video filename contains no valid YYYYMMDD_HHMMSS timestamp')
    applyFilenameCreationTime(draft, video, creationTime)
  } else if (source === 'detected') {
    resetCreationTime(draft, video)
  } else {
    throw new Error(`Unknown creation time source: ${source}`)
  }
  placeVideoIfAutomatic(draft, videoId)
}

/** Uses the `YYYYMMDD_HHMMSS` filename timestamp of every clip that has one, then detects times again. */
export function applyFilenameCreationTimes(draft) {
  const filenameTimes = draft.importedVideos.map((video) => parseVideoFilenameCreationTime(video.path))
  if (filenameTimes.every((creationTime) => creationTime === null)) {
    throw new Error('No video filename contains a valid YYYYMMDD_HHMMSS timestamp')
  }
  draft.importedVideos.forEach((video, index) => {
    if (filenameTimes[index] !== null) applyFilenameCreationTime(draft, video, filenameTimes[index])
  })
  placeAutomaticVideos(draft)
}

/** Returns every clip to its detected creation time, then detects times again. */
export function resetCreationTimes(draft) {
  for (const video of draft.importedVideos) resetCreationTime(draft, video)
  placeAutomaticVideos(draft)
}

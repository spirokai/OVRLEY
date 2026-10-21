/**
 * Edits of the imported-video collection, shared by the store and the import
 * dialog's draft. Each recipe mutates an Immer draft of the collection
 * `{ importedVideos, videoEdits, videoSyncTimezoneMode, parsedActivity, activitySummary }`.
 *
 * Placement: an `auto` clip follows its detected recording time (against the
 * external activity, otherwise relative to the earliest clip) plus the shared
 * correction of the collection; a clip whose time cannot be determined sits next
 * to its placement anchor. A `manual` clip keeps the start set by hand when times
 * are detected again. Shifts move every clip and keep their relative placement.
 */

import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { placeVideosByCreationTime, resolveVideosSync } from '@/lib/video-sync'
import { createVideoEdits } from './imported-video'
import { joinVideoEdits, sortVideosByTimeline } from './video-collection'

/**
 * Returns the timeline start that puts a clip directly before or after its anchor clip.
 * @param {{durationSeconds: number}} video Placed clip.
 * @param {{offsetSeconds: number, durationSeconds: number}} anchor Anchor clip with its committed offset.
 * @param {'before'|'after'} side Side of the anchor.
 * @returns {number} Timeline start second.
 */
export function anchoredOffsetSeconds(video, anchor, side) {
  return side === 'before' ? anchor.offsetSeconds - video.durationSeconds : anchor.offsetSeconds + anchor.durationSeconds
}

function findVideo(collection, videoId) {
  const video = collection.importedVideos.find((candidate) => candidate.id === videoId)
  if (video === undefined) throw new Error(`Unknown imported video: ${videoId}`)
  return video
}

function requireFiniteOffset(seconds, label) {
  if (!Number.isFinite(seconds)) throw new Error(`${label} must be a finite number`)
}

/**
 * Detects the placement of every given clip: against the external activity when
 * one is loaded, otherwise at detected real time relative to the earliest clip.
 * @returns {{placements: Array<{offsetSeconds: number, determined: boolean, syncWarning: string|null}>, timezoneMode: 'utc'|'local'|null}}
 *   Placements in input order and the resulting session timezone mode.
 */
function detectVideoPlacements(collection, videos) {
  if (collection.parsedActivity === null) return { placements: placeVideosByCreationTime(videos), timezoneMode: null }
  return resolveVideosSync(videos, collection.activitySummary, collection.videoSyncTimezoneMode)
}

/**
 * Lists the clips whose recording time cannot be determined.
 * @param {object} collection Collection state.
 * @returns {Set<string>} Video IDs.
 */
export function findUndeterminedVideoIds(collection) {
  const { placements } = detectVideoPlacements(collection, collection.importedVideos)
  return new Set(collection.importedVideos.filter((_, index) => !placements[index].determined).map((video) => video.id))
}

/** Moves a clip next to its anchor, then every automatic clip anchored to it, along anchor chains. */
function placeAtAnchors(draft, videoId) {
  const moved = [videoId]
  while (moved.length > 0) {
    const movedId = moved.shift()
    const { videoId: anchorId, side } = draft.videoEdits[movedId].placementAnchor
    draft.videoEdits[movedId].offsetSeconds = anchoredOffsetSeconds(
      findVideo(draft, movedId),
      { ...findVideo(draft, anchorId), ...draft.videoEdits[anchorId] },
      side,
    )
    moved.push(
      ...draft.importedVideos
        .filter((video) => {
          const edits = draft.videoEdits[video.id]
          return edits.placementAnchor?.videoId === movedId && edits.offsetSource === 'auto'
        })
        .map((video) => video.id),
    )
  }
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
  const { placements, timezoneMode } = detectVideoPlacements(draft, draft.importedVideos)
  const placementsById = new Map(draft.importedVideos.map((video, index) => [video.id, placements[index]]))
  const placed = new Set(videoIds)
  const references = sortVideosByTimeline(joinVideoEdits(draft.importedVideos, draft.videoEdits)).filter(
    (clip) => !placed.has(clip.id) && placementsById.get(clip.id).determined && (clip.offsetSource === 'auto' || draft.parsedActivity === null),
  )
  const reference = references.find((clip) => clip.offsetSource === 'auto') ?? references[0]
  const correctionSeconds = reference ? reference.offsetSeconds - placementsById.get(reference.id).offsetSeconds : 0
  for (const videoId of videoIds) {
    const placement = placementsById.get(videoId)
    findVideo(draft, videoId).syncWarning = placement.syncWarning
    draft.videoEdits[videoId].offsetSeconds = placement.determined ? placement.offsetSeconds + correctionSeconds : placement.offsetSeconds
  }
  draft.videoSyncTimezoneMode = timezoneMode
  return videoIds.filter((videoId) => !placementsById.get(videoId).determined)
}

/**
 * Adds videos not yet in the collection. Existing clips keep their placement; new
 * clips are placed like the collection (see `placeLikeTheCollection`). A new clip
 * whose time cannot be determined goes after the last placed clip.
 * @param {object} draft Collection draft.
 * @param {Array<object>} videos Imported video entities with paths not yet in the collection.
 * @returns {Array<string>} IDs of added clips whose time could not be determined, in input order.
 */
export function addVideos(draft, videos) {
  if (videos.length === 0) throw new Error('At least one video is required')
  const paths = new Set(draft.importedVideos.map((video) => video.path))
  for (const video of videos) {
    if (paths.has(video.path)) throw new Error(`Video is already imported: ${video.path}`)
    paths.add(video.path)
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
  placeAtAnchors(draft, videoId)
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
    placeAtAnchors(draft, occupant.id)
  }
  for (const follower of followers) {
    if (draft.videoEdits[follower.id].placementAnchor !== null) placeAtAnchors(draft, follower.id)
  }
}

/**
 * Places every automatic clip by detected time; a clip whose time cannot be
 * determined but that has a placement anchor sits next to its anchor instead.
 * Clips placed by hand keep their start.
 */
export function placeAutomaticVideos(draft) {
  if (draft.importedVideos.length === 0) return
  const { placements, timezoneMode } = detectVideoPlacements(draft, draft.importedVideos)
  const anchored = []
  draft.importedVideos.forEach((video, index) => {
    const edits = draft.videoEdits[video.id]
    if (edits.offsetSource === 'manual') return
    if (!placements[index].determined && edits.placementAnchor !== null) {
      anchored.push(video.id)
      return
    }
    edits.offsetSeconds = placements[index].offsetSeconds
    video.syncWarning = placements[index].syncWarning
  })
  // Anchors are resolved after the clips they point to (anchors never form cycles).
  const resolved = new Set(draft.importedVideos.map((video) => video.id).filter((videoId) => !anchored.includes(videoId)))
  while (anchored.length > 0) {
    const index = anchored.findIndex((videoId) => resolved.has(draft.videoEdits[videoId].placementAnchor.videoId))
    if (index === -1) throw new Error('Video placement anchors must reference imported videos')
    const [videoId] = anchored.splice(index, 1)
    const { videoId: anchorId, side } = draft.videoEdits[videoId].placementAnchor
    draft.videoEdits[videoId].offsetSeconds = anchoredOffsetSeconds(
      findVideo(draft, videoId),
      { ...findVideo(draft, anchorId), ...draft.videoEdits[anchorId] },
      side,
    )
    findVideo(draft, videoId).syncWarning = null
    resolved.add(videoId)
  }
  draft.videoSyncTimezoneMode = timezoneMode
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
  const edits = draft.videoEdits[videoId]
  edits.offsetSource = 'auto'
  const [undetermined] = placeLikeTheCollection(draft, [videoId])
  if (undetermined !== undefined && edits.placementAnchor !== null) {
    findVideo(draft, videoId).syncWarning = null
    placeAtAnchors(draft, videoId)
  }
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

/** Interprets ambiguous camera clock times in local time or UTC, then detects times again. */
export function setVideoSyncTimezoneMode(draft, mode) {
  if (mode !== 'local' && mode !== 'utc') throw new Error('Video sync timezone mode must be local or utc')
  draft.videoSyncTimezoneMode = mode
  placeAutomaticVideos(draft)
}

/** Uses the `YYYYMMDD_HHMMSS` filename timestamp of every clip that has one, then detects times again. */
export function applyFilenameCreationTimes(draft) {
  const filenameTimes = draft.importedVideos.map((video) => parseVideoFilenameCreationTime(video.path))
  if (filenameTimes.every((creationTime) => creationTime === null)) {
    throw new Error('No video filename contains a valid YYYYMMDD_HHMMSS timestamp')
  }
  draft.importedVideos.forEach((video, index) => {
    if (filenameTimes[index] === null) return
    video.creationTime = filenameTimes[index]
    video.timeSource = 'filename'
  })
  draft.videoSyncTimezoneMode = 'local'
  placeAutomaticVideos(draft)
}

/** Returns every clip to its detected creation time, then detects times again. */
export function resetCreationTimes(draft) {
  for (const video of draft.importedVideos) {
    video.creationTime = video.detectedCreationTime
    video.timeSource = video.detectedTimeSource
  }
  draft.videoSyncTimezoneMode = null
  placeAutomaticVideos(draft)
}

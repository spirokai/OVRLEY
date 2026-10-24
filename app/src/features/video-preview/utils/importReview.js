/**
 * Pure review of a video import: which videos start far from the activity (or
 * from the other videos), which are switched on when the dialog opens, the
 * collection the switched-on videos form, and the rows of the import dialog in
 * timeline order.
 */

import { filenameFromSelectedPath } from '@/lib/utils'
import {
  addVideos,
  anchorVideo,
  findUndeterminedVideoIds,
  moveVideoNextTo,
  removeVideo,
  setVideoSyncTimezoneMode,
} from '@/lib/video-program/collection-edits'
import { findCombinedExportIssues } from '@/lib/video-program/combined-export'
import { buildProgram } from '@/lib/video-program/program'
import { FULL_EXPORT_RANGE } from '@/lib/video-program/timeline'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { produce } from 'immer'

/** How far before the activity (or the videos already imported) a video may start. */
export const IMPORT_WINDOW_SECONDS = 3600
/** Without an activity or videos already imported, the videos of an import should start within this span. */
export const FRESH_IMPORT_SPAN_SECONDS = 12 * 3600

/**
 * Finds the largest group of clips that start within a span of each other; the
 * earliest group wins a tie.
 * @param {Array<{offsetSeconds: number}>} clips Clips.
 * @param {number} spanSeconds Longest distance between the starts of a group.
 * @returns {{from: number, to: number}|null} First and last start of the group, or null without clips.
 */
export function findLargestStartGroup(clips, spanSeconds) {
  const starts = clips.map((clip) => clip.offsetSeconds).sort((a, b) => a - b)
  let best = null
  let first = 0
  for (let last = 0; last < starts.length; last += 1) {
    while (starts[last] - starts[first] > spanSeconds) first += 1
    if (best === null || last - first > best.size) best = { size: last - first, from: starts[first], to: starts[last] }
  }
  return best === null ? null : { from: best.from, to: best.to }
}

/**
 * Finds the clips that start far from the reference of the import. Only the start of a
 * video counts, and a new video whose time cannot be determined counts once the user placed it:
 * - with an activity, a video starts more than an hour before the activity start or after its end;
 * - without one, a new video starts more than an hour before the first switched-on video already
 *   in the collection or after the end of the last one;
 * - without either, a video starts more than 12 hours away from the largest group of switched-on
 *   videos that start within 12 hours of each other.
 * @param {object} options Review inputs.
 * @param {Array<object>} options.clips Clips (videos joined with their edits).
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Set<string>} options.includedIds Videos switched on.
 * @param {Set<string>} options.unplacedIds Videos without a start to check (see `resolveImportSwitches`).
 * @param {number|null} options.activityDurationSeconds External activity duration, or null without an activity.
 * @returns {{kind: 'activity'|'videos'|'span', farIds: Set<string>}} The rule that applied and the clips far from its reference.
 */
export function findFarVideos({ clips, newIds, includedIds, unplacedIds, activityDurationSeconds }) {
  const placed = clips.filter((clip) => !unplacedIds.has(clip.id))
  const startsOutside = (from, to) => (clip) => clip.offsetSeconds < from || clip.offsetSeconds > to
  if (activityDurationSeconds !== null) {
    const outside = startsOutside(-IMPORT_WINDOW_SECONDS, activityDurationSeconds + IMPORT_WINDOW_SECONDS)
    return { kind: 'activity', farIds: new Set(placed.filter(outside).map((clip) => clip.id)) }
  }
  const reference = clips.filter((clip) => !newIds.has(clip.id) && includedIds.has(clip.id))
  if (reference.length > 0) {
    const outside = startsOutside(
      Math.min(...reference.map((clip) => clip.offsetSeconds)) - IMPORT_WINDOW_SECONDS,
      Math.max(...reference.map((clip) => clip.offsetSeconds + clip.durationSeconds)) + IMPORT_WINDOW_SECONDS,
    )
    return { kind: 'videos', farIds: new Set(placed.filter((clip) => newIds.has(clip.id) && outside(clip)).map((clip) => clip.id)) }
  }
  const group = findLargestStartGroup(
    placed.filter((clip) => includedIds.has(clip.id)),
    FRESH_IMPORT_SPAN_SECONDS,
  )
  const outside = (clip) => Math.max(group.to, clip.offsetSeconds) - Math.min(group.from, clip.offsetSeconds) > FRESH_IMPORT_SPAN_SECONDS
  return { kind: 'span', farIds: new Set(group === null ? [] : placed.filter(outside).map((clip) => clip.id)) }
}

/**
 * Lists the videos switched on by default: every video already in the collection, and every new
 * video with a start (its recording time, or a place the user gave it) near the reference (see
 * `findFarVideos`; without a reference, the largest group of new videos within 12 hours).
 * @param {object} options Review inputs.
 * @param {Array<object>} options.clips Clips (videos joined with their edits).
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Set<string>} options.unplacedIds Videos without a start to check (see `resolveImportSwitches`).
 * @param {number|null} options.activityDurationSeconds External activity duration, or null without an activity.
 * @returns {Set<string>} Video IDs.
 */
export function findDefaultIncludedIds({ clips, newIds, unplacedIds, activityDurationSeconds }) {
  const candidates = new Set(clips.filter((clip) => !unplacedIds.has(clip.id)).map((clip) => clip.id))
  const { farIds } = findFarVideos({ clips, newIds, includedIds: candidates, unplacedIds, activityDurationSeconds })
  return new Set([...candidates].filter((videoId) => !newIds.has(videoId) || !farIds.has(videoId)))
}

function activityDurationOf(collection) {
  return collection.parsedActivity === null ? null : collection.activitySummary.durationSeconds
}

/**
 * Finds the new videos that cannot be imported because the overlay is laid out for another
 * resolution: that of the largest group of videos already imported with one resolution, or in a
 * fresh import of the largest group of new videos; a tie goes to the earliest-imported video.
 * @param {object} options Review state.
 * @param {object} options.collection Draft collection.
 * @param {Set<string>} options.newIds Videos of this import.
 * @returns {{resolution: {width: number, height: number}|null, blockedIds: Set<string>}} Resolution of the overlay (null without
 *   videos) and the new videos with another one.
 */
export function findOtherResolutionVideos({ collection, newIds }) {
  const existing = collection.importedVideos.filter((video) => !newIds.has(video.id))
  const groups = []
  for (const video of [...(existing.length > 0 ? existing : collection.importedVideos)].sort((a, b) => a.importOrder - b.importOrder)) {
    const group = groups.find(([first]) => first.resolution.width === video.resolution.width && first.resolution.height === video.resolution.height)
    if (group) group.push(video)
    else groups.push([video])
  }
  if (groups.length === 0) return { resolution: null, blockedIds: new Set() }
  const { resolution } = groups.reduce((best, group) => (group.length > best.length ? group : best))[0]
  const blockedIds = collection.importedVideos
    .filter((video) => newIds.has(video.id) && (video.resolution.width !== resolution.width || video.resolution.height !== resolution.height))
    .map((video) => video.id)
  return { resolution: { ...resolution }, blockedIds: new Set(blockedIds) }
}

/**
 * Resolves the switches of the import dialog: the defaults for the current placement (see
 * `findDefaultIncludedIds`), so detecting times again (timezone, filename times) updates them,
 * except for the videos whose switch the user set or that the user edited. A new video whose
 * time cannot be determined waits switched off until the user switches it on; from then on it
 * is placed and follows the defaults like the others. A new video with another resolution than
 * the overlay (see `findOtherResolutionVideos`) is always off.
 * @param {object} options Review state.
 * @param {object} options.collection Draft collection.
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Map<string, boolean>} options.switchOverrides Switches set by the user (an edited video counts as switched on).
 * @param {Set<string>} options.placedIds New videos whose time cannot be determined that the user switched on once.
 * @returns {{includedIds: Set<string>, undeterminedIds: Set<string>, unplacedIds: Set<string>, blockedIds: Set<string>,
 *   resolution: {width: number, height: number}|null}} Videos switched on, those whose time cannot be determined, the new ones
 *   of them still waiting, those with another resolution, and the overlay resolution.
 */
export function resolveImportSwitches({ collection, newIds, switchOverrides, placedIds }) {
  const undeterminedIds = findUndeterminedVideoIds(collection)
  const unplacedIds = new Set([...undeterminedIds].filter((videoId) => newIds.has(videoId) && !placedIds.has(videoId)))
  const { resolution, blockedIds } = findOtherResolutionVideos({ collection, newIds })
  const clips = joinVideoEdits(collection.importedVideos, collection.videoEdits)
  const includedIds = findDefaultIncludedIds({ clips, newIds, unplacedIds, activityDurationSeconds: activityDurationOf(collection) })
  for (const [videoId, isIncluded] of switchOverrides) {
    if (isIncluded) includedIds.add(videoId)
    else includedIds.delete(videoId)
  }
  for (const videoId of blockedIds) includedIds.delete(videoId)
  return { includedIds, undeterminedIds, unplacedIds, blockedIds, resolution }
}

/**
 * Places the draft as the dialog shows it and imports it, without changing the draft, so every
 * change (timezone, switches) can be undone by reverting it:
 * - a switched-on automatic video placed next to a switched-off one sits right after the
 *   switched-on video above it (it keeps its start without one), so switched-off videos never
 *   leave a gap;
 * - new videos whose time cannot be determined and that wait to be switched on wait one after
 *   another after the last switched-on video.
 * @param {object} options Review state.
 * @param {object} options.collection Draft collection.
 * @param {Set<string>} options.includedIds Videos switched on (see `resolveImportSwitches`).
 * @param {Set<string>} options.unplacedIds New videos waiting to be switched on (see `resolveImportSwitches`).
 * @returns {object} Collection as shown.
 */
export function buildShownCollection({ collection, includedIds, unplacedIds }) {
  return produce(collection, (draft) => {
    const included = () => sortVideosByTimeline(joinVideoEdits(draft.importedVideos, draft.videoEdits)).filter((clip) => includedIds.has(clip.id))
    included().forEach((clip, index, order) => {
      const anchor = clip.placementAnchor
      if (clip.offsetSource !== 'auto' || anchor === null || includedIds.has(anchor.videoId)) return
      if (index > 0) anchorVideo(draft, clip.id, order[index - 1].id, 'after')
      else draft.videoEdits[clip.id].placementAnchor = null
    })
    const lastIncluded = included().at(-1)
    if (lastIncluded === undefined) return
    let previousId = lastIncluded.id
    for (const video of collection.importedVideos) {
      if (!unplacedIds.has(video.id)) continue
      anchorVideo(draft, video.id, previousId, 'after')
      previousId = video.id
    }
  })
}

/**
 * Places a waiting video where the dialog shows it: right after the switched-on video above it,
 * or at its shown start without one.
 * @param {object} draft Collection draft.
 * @param {string} videoId Waiting video.
 * @param {object} shown Collection as shown (see `buildShownCollection`).
 * @param {Set<string>} includedIds Videos switched on before.
 */
function placeWaitingVideoInDraft(draft, videoId, shown, includedIds) {
  const order = sortVideosByTimeline(joinVideoEdits(shown.importedVideos, shown.videoEdits))
  const index = order.findIndex((clip) => clip.id === videoId)
  const above = order.slice(0, index).filter((clip) => includedIds.has(clip.id))
  if (above.length > 0) {
    moveVideoNextTo(draft, videoId, above.at(-1).id, 'after')
    return
  }
  draft.videoEdits[videoId].placementAnchor = null
  draft.videoEdits[videoId].offsetSeconds = order[index].offsetSeconds
}

/**
 * Switches on a new video whose time cannot be determined: it stays where it waited and from then
 * on follows the defaults like any other video (see `resolveImportSwitches`). Switched on where it
 * would start far from the reference, its switch stays on as the user set it.
 * @param {object} review Review state (see `resolveImportSwitches`).
 * @param {string} videoId Waiting video.
 * @param {object} shown Collection as shown (see `buildShownCollection`).
 * @param {Set<string>} includedIds Videos switched on before.
 * @returns {{collection: object, placedIds: Set<string>, switchOverrides: Map<string, boolean>}} Changed review state.
 */
export function switchOnWaitingVideo(review, videoId, shown, includedIds) {
  const next = {
    ...review,
    collection: produce(review.collection, (draft) => placeWaitingVideoInDraft(draft, videoId, shown, includedIds)),
    placedIds: new Set([...review.placedIds, videoId]),
  }
  const switchOverrides = resolveImportSwitches(next).includedIds.has(videoId)
    ? review.switchOverrides
    : new Map([...review.switchOverrides, [videoId, true]])
  return { collection: next.collection, placedIds: next.placedIds, switchOverrides }
}

/**
 * Reads the camera clocks (`ffprobe` and `filename` times) of a collection as local time or as
 * UTC, whichever puts more videos near the activity (see `findFarVideos`). A tie keeps the
 * reading each video inferred for itself.
 * @param {object} collection Collection with an activity.
 * @returns {object} Collection placed with the chosen reading.
 */
export function chooseTimezoneMode(collection) {
  const nearCount = (candidate) => {
    const clips = joinVideoEdits(candidate.importedVideos, candidate.videoEdits)
    const undeterminedIds = findUndeterminedVideoIds(candidate)
    const { farIds } = findFarVideos({
      clips,
      newIds: new Set(),
      includedIds: new Set(),
      unplacedIds: undeterminedIds,
      activityDurationSeconds: activityDurationOf(candidate),
    })
    return clips.filter((clip) => !undeterminedIds.has(clip.id) && !farIds.has(clip.id)).length
  }
  const [local, utc] = ['local', 'utc'].map((mode) => produce(collection, (draft) => setVideoSyncTimezoneMode(draft, mode)))
  const [localCount, utcCount] = [nearCount(local), nearCount(utc)]
  if (localCount === utcCount) return collection
  return localCount > utcCount ? local : utc
}

/**
 * Opens the review of an import: adds the probed videos to a draft of the collection with every
 * switch at its default. A first import
 * against an activity reads the camera clocks the way that fits the activity best (see
 * `chooseTimezoneMode`); adding videos keeps the reading of the collection.
 * @param {object} collection Collection the videos are added to.
 * @param {Array<object>} videos Probed videos, in selection order.
 * @returns {{collection: object, newIds: Set<string>, switchOverrides: Map<string, boolean>, placedIds: Set<string>}} Review state.
 */
export function openImportReview(collection, videos) {
  const withVideos = produce(collection, (draft) => {
    if (videos.length > 0) addVideos(draft, videos)
  })
  const isFirstImport = collection.importedVideos.length === 0 && videos.length > 0
  const added = isFirstImport && collection.parsedActivity !== null ? chooseTimezoneMode(withVideos) : withVideos
  return { collection: added, newIds: new Set(videos.map((video) => video.id)), switchOverrides: new Map(), placedIds: new Set() }
}

/**
 * Builds the collection of the switched-on videos.
 * @param {object} collection Collection as shown (see `buildShownCollection`).
 * @param {Set<string>} includedIds Videos switched on.
 * @returns {object} Collection of the switched-on videos.
 */
export function buildIncludedCollection(collection, includedIds) {
  return produce(collection, (draft) => {
    for (const video of collection.importedVideos) if (!includedIds.has(video.id)) removeVideo(draft, video.id)
  })
}

/**
 * Builds the rows of the import dialog in timeline order, with the videos each overlaps among
 * the switched-on ones, whether it starts far from the reference, and the single-file issues.
 * @param {object} options Review inputs.
 * @param {object} options.collection Collection as shown (see `buildShownCollection`).
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Set<string>} options.includedIds Videos switched on.
 * @param {Set<string>} options.undeterminedIds Videos whose recording time cannot be determined.
 * @param {Set<string>} options.unplacedIds New videos waiting to be switched on.
 * @param {Set<string>} options.blockedIds New videos with another resolution than the overlay.
 * @param {{type: string, durationSeconds: number, splitGapThresholdSeconds: number}} options.transitionDefaults Draft transition defaults.
 * @returns {{windowKind: 'activity'|'videos'|'span', rows: Array<object>, included: object, issues: Array<object>,
 *   timeline: {fromSecond: number, toSecond: number}|null}} The rule that applied, dialog rows, the collection of the switched-on
 *   videos, single-file issues (see `findCombinedExportIssues`), and the timeline they span with the activity (null without videos).
 */
export function buildImportReview({ collection, newIds, includedIds, undeterminedIds, unplacedIds, blockedIds, transitionDefaults }) {
  const included = buildIncludedCollection(collection, includedIds)
  const includedClips = sortVideosByTimeline(joinVideoEdits(included.importedVideos, included.videoEdits))
  const clips = sortVideosByTimeline(joinVideoEdits(collection.importedVideos, collection.videoEdits))
  const activityDurationSeconds = activityDurationOf(collection)
  const { kind, farIds } = findFarVideos({ clips, newIds, includedIds, unplacedIds, activityDurationSeconds })
  const program = buildProgram({ videos: includedClips, exportRange: FULL_EXPORT_RANGE, transitionDefaults })
  const names = new Map(clips.map((clip) => [clip.id, filenameFromSelectedPath(clip.path)]))
  const overlapsOf = (videoId) =>
    program.overlaps
      .filter((overlap) => overlap.earlierVideoId === videoId || overlap.laterVideoId === videoId)
      .map((overlap) => ({
        name: names.get(overlap.earlierVideoId === videoId ? overlap.laterVideoId : overlap.earlierVideoId),
        seconds: overlap.endSecond - overlap.startSecond,
      }))
  const issues = findCombinedExportIssues(program, includedClips)
  const incompatibleIds = new Set(issues.filter((issue) => issue.kind === 'incompatible').flatMap((issue) => issue.videoIds))
  const rows = clips.map((clip) => ({
    id: clip.id,
    filename: names.get(clip.id),
    path: clip.path,
    isNew: newIds.has(clip.id),
    isIncluded: includedIds.has(clip.id),
    hasOtherResolution: blockedIds.has(clip.id),
    isWaiting: unplacedIds.has(clip.id),
    isFar: farIds.has(clip.id),
    offsetSeconds: clip.offsetSeconds,
    fps: clip.fps,
    resolution: clip.resolution,
    durationSeconds: clip.durationSeconds,
    isManual: clip.offsetSource === 'manual',
    // Only a new video whose time cannot be determined is positioned in the list.
    isPositioned: newIds.has(clip.id) && undeterminedIds.has(clip.id),
    isIncompatible: incompatibleIds.has(clip.id),
    overlaps: includedIds.has(clip.id) ? overlapsOf(clip.id) : [],
  }))
  const bounds = [
    ...includedClips.flatMap((clip) => [clip.offsetSeconds, clip.offsetSeconds + clip.durationSeconds]),
    ...(activityDurationSeconds === null || includedClips.length === 0 ? [] : [0, activityDurationSeconds]),
  ]
  const timeline = bounds.length === 0 ? null : { fromSecond: Math.min(...bounds), toSecond: Math.max(...bounds) }
  return { windowKind: kind, rows, included, issues, timeline }
}

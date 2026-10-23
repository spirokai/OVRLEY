/**
 * Pure review of a video import: which videos start far from the activity (or
 * from the other videos), which are switched on when the dialog opens, the
 * collection the switched-on videos form, and the rows of the import dialog in
 * timeline order.
 */

import { filenameFromSelectedPath } from '@/lib/utils'
import { addVideos, anchorVideo, findUndeterminedVideoIds, removeVideo } from '@/lib/video-program/collection-edits'
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
 * video counts, and a video whose time cannot be determined is placed by the user:
 * - with an activity, a video starts more than an hour before the activity start or after its end;
 * - without one, a new video starts more than an hour before the first switched-on video already
 *   in the collection or after the end of the last one;
 * - without either, a video starts more than 12 hours away from the largest group of switched-on
 *   videos that start within 12 hours of each other.
 * @param {object} options Review inputs.
 * @param {Array<object>} options.clips Clips (videos joined with their edits).
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Set<string>} options.includedIds Videos switched on.
 * @param {Set<string>} options.undeterminedIds Videos whose recording time cannot be determined.
 * @param {number|null} options.activityDurationSeconds External activity duration, or null without an activity.
 * @returns {{kind: 'activity'|'videos'|'span', farIds: Set<string>}} The rule that applied and the clips far from its reference.
 */
export function findFarVideos({ clips, newIds, includedIds, undeterminedIds, activityDurationSeconds }) {
  const placed = clips.filter((clip) => !undeterminedIds.has(clip.id))
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
 * Lists the videos switched on when the import dialog opens: every video already in the
 * collection, and every new video whose time is known and that starts near the reference
 * (see `findFarVideos`; without a reference, the largest group of new videos within 12 hours).
 * @param {object} options Review inputs.
 * @param {Array<object>} options.clips Clips (videos joined with their edits).
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Set<string>} options.undeterminedIds Videos whose recording time cannot be determined.
 * @param {number|null} options.activityDurationSeconds External activity duration, or null without an activity.
 * @returns {Set<string>} Video IDs.
 */
export function findDefaultIncludedIds({ clips, newIds, undeterminedIds, activityDurationSeconds }) {
  const candidates = new Set(clips.filter((clip) => !newIds.has(clip.id) || !undeterminedIds.has(clip.id)).map((clip) => clip.id))
  const { farIds } = findFarVideos({ clips, newIds, includedIds: candidates, undeterminedIds, activityDurationSeconds })
  return new Set([...candidates].filter((videoId) => !newIds.has(videoId) || !farIds.has(videoId)))
}

function activityDurationOf(collection) {
  return collection.parsedActivity === null ? null : collection.activitySummary.durationSeconds
}

/**
 * Resolves the switches of the import dialog: the defaults for the current placement (see
 * `findDefaultIncludedIds`), so detecting times again (timezone, filename times) updates them,
 * except for the videos whose switch the user set or that the user edited.
 * @param {object} options Review state.
 * @param {object} options.collection Draft collection.
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Map<string, boolean>} options.switchOverrides Switches set by the user (an edited video counts as switched on).
 * @returns {{includedIds: Set<string>, undeterminedIds: Set<string>}} Videos switched on, and those whose time cannot be determined.
 */
export function resolveImportSwitches({ collection, newIds, switchOverrides }) {
  const undeterminedIds = findUndeterminedVideoIds(collection)
  const clips = joinVideoEdits(collection.importedVideos, collection.videoEdits)
  const includedIds = findDefaultIncludedIds({ clips, newIds, undeterminedIds, activityDurationSeconds: activityDurationOf(collection) })
  for (const [videoId, isIncluded] of switchOverrides) {
    if (isIncluded) includedIds.add(videoId)
    else includedIds.delete(videoId)
  }
  return { includedIds, undeterminedIds }
}

/**
 * Keeps the placement of the draft consistent with its switches after every change:
 * - a switched-on automatic video placed next to a switched-off one moves right after the
 *   switched-on video above it (it keeps its start without one), so switched-off videos never
 *   leave a gap;
 * - new videos whose time cannot be determined and whose switch follows the defaults (so they
 *   are off) wait one after another after the last switched-on video.
 * @param {object} options Review state (see `resolveImportSwitches`).
 * @returns {object} Settled draft collection.
 */
export function settleImportReview({ collection, newIds, switchOverrides }) {
  const { includedIds, undeterminedIds } = resolveImportSwitches({ collection, newIds, switchOverrides })
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
      if (!newIds.has(video.id) || !undeterminedIds.has(video.id) || switchOverrides.has(video.id)) continue
      anchorVideo(draft, video.id, previousId, 'after')
      previousId = video.id
    }
  })
}

/**
 * Opens the review of an import: adds the probed videos to a draft of the collection and
 * settles it (see `settleImportReview`) with every switch at its default.
 * @param {object} collection Collection the videos are added to.
 * @param {Array<object>} videos Probed videos, in selection order.
 * @returns {{collection: object, newIds: Set<string>, switchOverrides: Map<string, boolean>}} Review state.
 */
export function openImportReview(collection, videos) {
  const added = produce(collection, (draft) => {
    if (videos.length > 0) addVideos(draft, videos)
  })
  const review = { collection: added, newIds: new Set(videos.map((video) => video.id)), switchOverrides: new Map() }
  return { ...review, collection: settleImportReview(review) }
}

/**
 * Builds the collection of the switched-on videos of a settled draft.
 * @param {object} collection Settled draft collection.
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
 * @param {object} options.collection Settled draft collection.
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Set<string>} options.includedIds Videos switched on.
 * @param {Set<string>} options.undeterminedIds Videos whose recording time cannot be determined.
 * @param {{type: string, durationSeconds: number, splitGapThresholdSeconds: number}} options.transitionDefaults Draft transition defaults.
 * @returns {{windowKind: 'activity'|'videos'|'span', rows: Array<object>, included: object, issues: Array<object>,
 *   timeline: {fromSecond: number, toSecond: number}|null}} The rule that applied, dialog rows, the collection of the switched-on
 *   videos, single-file issues (see `findCombinedExportIssues`), and the timeline they span with the activity (null without videos).
 */
export function buildImportReview({ collection, newIds, includedIds, undeterminedIds, transitionDefaults }) {
  const included = buildIncludedCollection(collection, includedIds)
  const includedClips = sortVideosByTimeline(joinVideoEdits(included.importedVideos, included.videoEdits))
  const clips = sortVideosByTimeline(joinVideoEdits(collection.importedVideos, collection.videoEdits))
  const activityDurationSeconds = activityDurationOf(collection)
  const { kind, farIds } = findFarVideos({ clips, newIds, includedIds, undeterminedIds, activityDurationSeconds })
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
    isFar: farIds.has(clip.id),
    offsetSeconds: clip.offsetSeconds,
    fps: clip.fps,
    resolution: clip.resolution,
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

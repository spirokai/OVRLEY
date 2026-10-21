/**
 * Pure review of a video import: which videos join the collection, and the rows
 * of the import dialog in timeline order.
 */

import { filenameFromSelectedPath } from '@/lib/utils'
import { findCombinedExportIssues } from '@/lib/video-program/combined-export'
import { buildProgram } from '@/lib/video-program/program'
import { FULL_EXPORT_RANGE } from '@/lib/video-program/timeline'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'

/** How far before the activity (or the videos already imported) a video may start. */
export const IMPORT_WINDOW_SECONDS = 3600
/** Without an activity, a fresh import takes its videos only when they start within this span. */
export const FRESH_IMPORT_SPAN_SECONDS = 12 * 3600

/**
 * Decides which clips of an import join the collection. Only the start of a video counts:
 * - with an activity, a video starts at most an hour before the activity start or after its end;
 * - without one, a new video starts at most an hour before the first video already in the
 *   collection or after the end of the last one;
 * - a fresh import without an activity takes every video when they all start within 12 hours,
 *   otherwise none until the sync values are edited.
 * Videos already in the collection always stay; one that starts outside the activity window is
 * flagged. A new video whose time cannot be determined is placed by the user and joins.
 * @param {object} options Review inputs.
 * @param {Array<object>} options.clips Draft clips (videos joined with their edits).
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Set<string>} options.undeterminedIds Videos whose recording time cannot be determined.
 * @param {number|null} options.activityDurationSeconds External activity duration, or null without an activity.
 * @returns {{kind: 'activity'|'videos'|'span', verdicts: Map<string, 'import'|'excluded'|'kept'|'outside'>}} The rule that
 *   applied and the verdict of every clip.
 */
export function reviewImportWindow({ clips, newIds, undeterminedIds, activityDurationSeconds }) {
  const existing = clips.filter((clip) => !newIds.has(clip.id))
  const placedNew = clips.filter((clip) => newIds.has(clip.id) && !undeterminedIds.has(clip.id))
  const verdicts = new Map()
  if (activityDurationSeconds === null && existing.length === 0) {
    const starts = placedNew.map((clip) => clip.offsetSeconds)
    const fits = starts.length === 0 || Math.max(...starts) - Math.min(...starts) <= FRESH_IMPORT_SPAN_SECONDS
    for (const clip of clips) verdicts.set(clip.id, fits ? 'import' : 'excluded')
    return { kind: 'span', verdicts }
  }
  const window =
    activityDurationSeconds === null
      ? {
          from: Math.min(...existing.map((clip) => clip.offsetSeconds)) - IMPORT_WINDOW_SECONDS,
          to: Math.max(...existing.map((clip) => clip.offsetSeconds + clip.durationSeconds)) + IMPORT_WINDOW_SECONDS,
        }
      : { from: -IMPORT_WINDOW_SECONDS, to: activityDurationSeconds + IMPORT_WINDOW_SECONDS }
  for (const clip of clips) {
    const inside = undeterminedIds.has(clip.id) || (clip.offsetSeconds >= window.from && clip.offsetSeconds <= window.to)
    if (newIds.has(clip.id)) verdicts.set(clip.id, inside ? 'import' : 'excluded')
    else verdicts.set(clip.id, inside ? 'kept' : 'outside')
  }
  return { kind: activityDurationSeconds === null ? 'videos' : 'activity', verdicts }
}

/**
 * Builds the rows of the import dialog in timeline order, with the verdict of each
 * video, the videos it overlaps among those that join, and the single-file issues.
 * @param {object} options Review inputs.
 * @param {object} options.collection Draft collection.
 * @param {Set<string>} options.newIds Videos of this import.
 * @param {Set<string>} options.undeterminedIds Videos whose recording time cannot be determined.
 * @param {{type: string, durationSeconds: number, splitGapThresholdSeconds: number}} options.transitionDefaults Draft transition defaults.
 * @returns {{windowKind: 'activity'|'videos'|'span', rows: Array<object>, joiningIds: Set<string>, issues: Array<object>}} The window
 *   rule that applied, dialog rows, the videos that join, and single-file issues (see `findCombinedExportIssues`).
 */
export function buildImportReview({ collection, newIds, undeterminedIds, transitionDefaults }) {
  const clips = sortVideosByTimeline(joinVideoEdits(collection.importedVideos, collection.videoEdits))
  const { kind, verdicts } = reviewImportWindow({
    clips,
    newIds,
    undeterminedIds,
    activityDurationSeconds: collection.parsedActivity === null ? null : collection.activitySummary.durationSeconds,
  })
  const joining = clips.filter((clip) => verdicts.get(clip.id) !== 'excluded')
  const program = buildProgram({ videos: joining, exportRange: FULL_EXPORT_RANGE, transitionDefaults })
  const names = new Map(clips.map((clip) => [clip.id, filenameFromSelectedPath(clip.path)]))
  const overlapsOf = (videoId) =>
    program.overlaps
      .filter((overlap) => overlap.earlierVideoId === videoId || overlap.laterVideoId === videoId)
      .map((overlap) => ({
        name: names.get(overlap.earlierVideoId === videoId ? overlap.laterVideoId : overlap.earlierVideoId),
        seconds: overlap.endSecond - overlap.startSecond,
      }))
  const issues = findCombinedExportIssues(program, joining)
  const incompatibleIds = new Set(issues.filter((issue) => issue.kind === 'incompatible').flatMap((issue) => issue.videoIds))
  const rows = clips.map((clip, index) => ({
    id: clip.id,
    filename: names.get(clip.id),
    path: clip.path,
    isNew: newIds.has(clip.id),
    verdict: verdicts.get(clip.id),
    offsetSeconds: clip.offsetSeconds,
    fps: clip.fps,
    resolution: clip.resolution,
    isManual: clip.offsetSource === 'manual',
    // Only a new video whose time cannot be determined is positioned in the list.
    isPositioned: newIds.has(clip.id) && undeterminedIds.has(clip.id),
    isFirst: index === 0,
    isLast: index === clips.length - 1,
    isIncompatible: incompatibleIds.has(clip.id),
    overlaps: verdicts.get(clip.id) === 'excluded' ? [] : overlapsOf(clip.id),
  }))
  return { windowKind: kind, rows, joiningIds: new Set(joining.map((clip) => clip.id)), issues }
}

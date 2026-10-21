/**
 * What prevents a set of videos from being exported as one combined file.
 */

import { deriveCompatibility } from './video-collection'

/**
 * Lists what prevents the videos from being exported as one file: overlapping
 * videos and videos whose resolution or frame rate differs from the others.
 * @param {{overlaps: Array<{earlierVideoId: string, laterVideoId: string}>}} program Program of the videos.
 * @param {Array<object>} clips Clips of the program.
 * @returns {Array<{kind: 'overlap'|'incompatible', videoIds: Array<string>}>} Issues; empty when one file is possible.
 */
export function findCombinedExportIssues(program, clips) {
  const issues = []
  if (program.overlaps.length > 0) {
    const overlapping = new Set(program.overlaps.flatMap((overlap) => [overlap.earlierVideoId, overlap.laterVideoId]))
    issues.push({ kind: 'overlap', videoIds: clips.filter((clip) => overlapping.has(clip.id)).map((clip) => clip.id) })
  }
  const { incompatibleIds } = deriveCompatibility(clips)
  if (incompatibleIds.size > 0)
    issues.push({ kind: 'incompatible', videoIds: clips.filter((clip) => incompatibleIds.has(clip.id)).map((clip) => clip.id) })
  return issues
}

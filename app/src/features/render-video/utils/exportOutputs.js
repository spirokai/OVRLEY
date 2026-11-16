/**
 * Reviews an export of the imported videos and builds its native batch outputs.
 *
 * Separate mode renders one output per video: the program of that video alone,
 * so overlaps with other videos do not trim it. Combined mode renders one output
 * holding the program of every video (overlaps resolved). Transparent outputs
 * join their pieces with hard cuts.
 */

import i18next from 'i18next'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import { findCombinedExportIssues } from '@/lib/video-program/combined-export'
import { buildProgram, findInvalidTransitions } from '@/lib/video-program/program'
import { HARD_CUT } from '@/lib/video-program/transition-catalog'
import { isQsvFullCodec } from './codecUtils'
import { reviewBatchQueue } from './renderPresentation'

/** Native output of a program: its pieces and the transitions of its joins. */
function nativeOutput(id, program, sourceIds, exportMode) {
  return {
    id,
    pieces: program.pieces.map((piece) => ({
      sourceId: sourceIds.get(piece.videoId),
      startFrame: piece.sourceStartFrame,
      endFrame: piece.sourceEndFrame,
      skipOverlay: piece.skipOverlay,
    })),
    transitions: program.joins.map((join) => (exportMode === 'transparent' ? { ...HARD_CUT } : { ...join.transition })),
  }
}

/**
 * Describes one reason the videos cannot be exported as one file (see `findCombinedExportIssues`).
 * @param {{kind: 'overlap'|'incompatible'|'rotation', videoIds: Array<string>}} issue Issue.
 * @param {Array<object>} clips Clips the issue refers to.
 * @param {import('i18next').TFunction} t Translation function.
 * @returns {string} Message naming the videos.
 */
export function formatCombinedExportIssue(issue, clips, t) {
  const names = clips.filter((clip) => issue.videoIds.includes(clip.id)).map((clip) => filenameFromSelectedPath(clip.path))
  switch (issue.kind) {
    case 'overlap':
      return t('render-video.combinedOverlap', {
        defaultValue: 'Videos overlap ({{names}}). Move a video or exclude the overlapping part from export.',
        names: names.join(', '),
      })
    case 'incompatible':
      return t('render-video.combinedIncompatible', {
        defaultValue: 'Videos differ in resolution or frame rate ({{names}}). Remove them or export separate files.',
        names: names.join(', '),
      })
    case 'rotation':
      return t('render-video.combinedRotation', {
        defaultValue: 'Videos are rotated differently ({{names}}), which this encoder cannot join into one file. Export separate files.',
        names: names.join(', '),
      })
    default:
      throw new Error(`Unknown combined export issue: ${issue.kind}`)
  }
}

function combinedBlockers({ program, clips, rows, exportMode, codec, t }) {
  const names = (ids) => clips.filter((clip) => ids.has(clip.id)).map((clip) => filenameFromSelectedPath(clip.path))
  const included = clips.filter((clip) => program.pieces.some((piece) => piece.videoId === clip.id))
  const blockers = []
  if (program.pieces.length === 0) blockers.push(t('render-video.nothingToExport', 'Every video part is excluded from export.'))
  // QSV full overlay keeps the source's display rotation, one per file.
  const keepsRotation = exportMode === 'composite' && isQsvFullCodec(codec)
  for (const issue of findCombinedExportIssues(program, included, { keepsRotation })) blockers.push(formatCombinedExportIssue(issue, clips, t))
  if (exportMode === 'composite' && findInvalidTransitions(program).length > 0) {
    blockers.push(t('render-video.combinedInvalidTransitions', 'Some transitions are longer than the video pieces they join.'))
  }
  const unavailable = new Set(rows.filter((row) => row.pieces.length > 0 && row.status === 'blocked').map((row) => row.id))
  if (unavailable.size > 0) {
    blockers.push(
      t('render-video.combinedUnavailable', { defaultValue: 'Some videos cannot be exported ({{names}}).', names: names(unavailable).join(', ') }),
    )
  }
  return blockers
}

/**
 * @param {object} options Review inputs.
 * @param {'combined'|'separate'} options.outputMode Output mode.
 * @param {'composite'|'transparent'} options.exportMode Export mode.
 * @param {string} options.codec Export codec.
 * @param {Array<object>} options.clips Timeline clips in timeline order.
 * @param {object} options.transitionDefaults Project transition defaults.
 * @param {object|null} options.inspection Current native descriptors keyed by path.
 * @param {object} options.sync Activity summary and display timezone mode.
 * @param {string} options.locale Display language.
 * @param {string} options.outputPath Combined output file (its output ID).
 * @returns {{rows: Array<object>, blockers: Array<string>, outputs: Array<object>|null, offsets: Object<string, number>}}
 *   Reviewed video rows, reasons combined export is unavailable, native outputs once every video is
 *   reviewed and nothing blocks them, and the committed offsets of their sources.
 */
export function reviewExport({ outputMode, exportMode, codec, clips, transitionDefaults, inspection, sync, locale, outputPath }) {
  const t = i18next.getFixedT(locale)
  const programOf = (videos) => buildProgram({ videos, transitionDefaults })
  const programs = outputMode === 'separate' ? new Map(clips.map((clip) => [clip.id, programOf([clip])])) : null
  const program = outputMode === 'combined' ? programOf(clips) : null
  const piecesByVideo = new Map(
    clips.map((clip) => [clip.id, (programs?.get(clip.id) ?? program).pieces.filter((piece) => piece.videoId === clip.id)]),
  )
  const rows = reviewBatchQueue(clips, piecesByVideo, inspection, sync, locale).map((row) =>
    outputMode === 'separate' && exportMode === 'composite' && row.status === 'pending' && findInvalidTransitions(programs.get(row.id)).length > 0
      ? { ...row, status: 'blocked', error: t('render-video.invalidTransitions', 'A transition is longer than the video pieces it joins') }
      : row,
  )
  const blockers = outputMode === 'combined' ? combinedBlockers({ program, clips, rows, exportMode, codec, t }) : []
  const pending = rows.filter((row) => row.status === 'pending')
  const sourceIds = new Map(pending.map((row) => [row.id, row.source.sourceId]))
  const settled = rows.every((row) => row.status !== 'checking')
  let outputs = null
  if (settled && blockers.length === 0) {
    outputs =
      outputMode === 'separate'
        ? pending.map((row) => nativeOutput(row.path, programs.get(row.id), sourceIds, exportMode))
        : [nativeOutput(outputPath, program, sourceIds, exportMode)]
  }
  const usedVideoIds = new Set((outputs ?? []).flatMap((output) => output.pieces.map((piece) => piece.sourceId)))
  const offsets = Object.fromEntries(
    pending.filter((row) => usedVideoIds.has(row.source.sourceId)).map((row) => [row.source.sourceId, row.offsetSeconds]),
  )
  return { rows, blockers, outputs, offsets }
}

/**
 * Reviews an export of the imported videos and builds its native batch outputs.
 *
 * Separate mode renders one output per video: its pieces after deleted parts and
 * the export range (overlaps with other videos do not trim them), joined by the
 * transitions at its deleted parts. Combined mode renders one output holding the
 * program (overlaps resolved), joined by the program's transitions. Transparent
 * outputs join their pieces with hard cuts.
 */

import i18next from 'i18next'
import { filenameFromSelectedPath } from '@/lib/utils'
import { buildProgram, buildVideoPieceTransitions, collectVideoPieces, findInvalidTransitions } from '@/lib/video-program/program'
import { HARD_CUT } from '@/lib/video-program/transition-catalog'
import { deriveCompatibility } from '@/lib/video-program/video-collection'
import { reviewBatchQueue } from './renderPresentation'

function nativePiece(piece, sourceId, skipOverlay) {
  return { sourceId, startFrame: piece.sourceStartFrame, endFrame: piece.sourceEndFrame, skipOverlay }
}

function nativeTransitions(transitions, exportMode) {
  return transitions.map((transition) => (exportMode === 'transparent' ? { ...HARD_CUT } : { ...transition }))
}

function hasInvalidTransitions(pieces, transitions) {
  return findInvalidTransitions({ pieces, joins: transitions.map((transition, index) => ({ key: String(index), transition })) }).length > 0
}

function groupPiecesByVideo(pieces) {
  const piecesByVideo = new Map()
  for (const piece of pieces) piecesByVideo.set(piece.videoId, [...(piecesByVideo.get(piece.videoId) ?? []), piece])
  return piecesByVideo
}

function combinedBlockers({ program, clips, rows, exportMode, t }) {
  const names = (ids) => clips.filter((clip) => ids.has(clip.id)).map((clip) => filenameFromSelectedPath(clip.path))
  const included = clips.filter((clip) => program.pieces.some((piece) => piece.videoId === clip.id))
  const blockers = []
  if (program.pieces.length === 0) blockers.push(t('render-video.nothingInExportRange', 'No video lies inside the export range.'))
  if (program.overlaps.length > 0) {
    const overlapping = new Set(program.overlaps.flatMap((overlap) => [overlap.earlierVideoId, overlap.laterVideoId]))
    blockers.push(
      t('render-video.combinedOverlap', {
        defaultValue: 'Videos overlap ({{names}}). Move a video or delete the overlapping part.',
        names: names(overlapping).join(', '),
      }),
    )
  }
  const { incompatibleIds } = deriveCompatibility(included)
  if (incompatibleIds.size > 0) {
    blockers.push(
      t('render-video.combinedIncompatible', {
        defaultValue: 'Videos differ in resolution or frame rate ({{names}}). Remove them or export separate files.',
        names: names(incompatibleIds).join(', '),
      }),
    )
  }
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
 * @param {Array<object>} options.clips Timeline clips in timeline order.
 * @param {{type: 'all'|'custom', from: number, to: number}} options.exportRange Export range in timeline seconds.
 * @param {object} options.transitionDefaults Project transition defaults.
 * @param {Object<string, boolean>} options.skipOverlay Video IDs that render without activity widgets.
 * @param {object|null} options.inspection Current native descriptors keyed by path.
 * @param {object} options.sync Activity summary and display timezone mode.
 * @param {string} options.locale Display language.
 * @param {string} options.outputPath Combined output file (its output ID).
 * @returns {{rows: Array<object>, blockers: Array<string>, outputs: Array<object>|null, offsets: Object<string, number>}}
 *   Reviewed video rows, reasons combined export is unavailable, native outputs once every video is
 *   reviewed and nothing blocks them, and the committed offsets of their sources.
 */
export function reviewExport({ outputMode, exportMode, clips, exportRange, transitionDefaults, skipOverlay, inspection, sync, locale, outputPath }) {
  const t = i18next.getFixedT(locale)
  const program = buildProgram({ videos: clips, exportRange, transitionDefaults })
  const piecesByVideo = outputMode === 'separate' ? collectVideoPieces({ videos: clips, exportRange }) : groupPiecesByVideo(program.pieces)
  const transitionsByVideo = new Map(
    clips.map((clip) => [
      clip.id,
      nativeTransitions(buildVideoPieceTransitions(clip, piecesByVideo.get(clip.id) ?? [], transitionDefaults), exportMode),
    ]),
  )
  const rows = reviewBatchQueue(clips, piecesByVideo, skipOverlay, inspection, sync, locale).map((row) =>
    outputMode === 'separate' && row.status === 'pending' && hasInvalidTransitions(row.pieces, transitionsByVideo.get(row.id))
      ? { ...row, status: 'blocked', error: t('render-video.invalidTransitions', 'A transition is longer than the video pieces it joins') }
      : row,
  )
  const blockers = outputMode === 'combined' ? combinedBlockers({ program, clips, rows, exportMode, t }) : []
  const sourceIds = new Map(rows.filter((row) => row.status === 'pending').map((row) => [row.id, row.source.sourceId]))
  const settled = rows.every((row) => row.status !== 'checking')
  let outputs = null
  if (settled && blockers.length === 0) {
    outputs =
      outputMode === 'separate'
        ? rows
            .filter((row) => row.status === 'pending')
            .map((row) => ({
              id: row.path,
              pieces: row.pieces.map((piece) => nativePiece(piece, row.source.sourceId, row.skipOverlay)),
              transitions: transitionsByVideo.get(row.id),
            }))
        : [
            {
              id: outputPath,
              pieces: program.pieces.map((piece) => nativePiece(piece, sourceIds.get(piece.videoId), skipOverlay[piece.videoId] === true)),
              transitions: nativeTransitions(
                program.joins.map((join) => join.transition),
                exportMode,
              ),
            },
          ]
  }
  const usedVideoIds = new Set((outputs ?? []).flatMap((output) => output.pieces.map((piece) => piece.sourceId)))
  const offsets = Object.fromEntries(
    rows.filter((row) => row.status === 'pending' && usedVideoIds.has(row.source.sourceId)).map((row) => [row.source.sourceId, row.offsetSeconds]),
  )
  return { rows, blockers, outputs, offsets }
}

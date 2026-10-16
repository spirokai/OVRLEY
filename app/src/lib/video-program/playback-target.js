/**
 * Resolves which imported video and source frame the preview shows at a timeline
 * second, and where playback continues.
 */

import { framesToSeconds } from './frame-grid'
import { resolveProgramPosition } from './program'

/**
 * Inside a piece (including its outgoing transition window) the piece's own frame
 * is shown. Inside a deleted part the deleted frame of that video is shown and
 * flagged. Before the first piece and inside gaps the upcoming piece's first frame
 * is shown; after the last piece its final frame is shown.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @param {number} second Timeline second.
 * @returns {{videoId: string, piece: object, sourceSecond: number, isInside: boolean, isDeleted: boolean}|null}
 *   Preview target (`piece` is the piece whose video is shown), or null without pieces.
 */
export function resolvePreviewTarget(program, second) {
  const position = resolveProgramPosition(program, second)
  switch (position.kind) {
    case 'piece':
    case 'transition':
      return { videoId: position.piece.videoId, piece: position.piece, sourceSecond: position.sourceSecond, isInside: true, isDeleted: false }
    case 'gap': {
      const { previousPiece, nextPiece, join } = position
      if (join.kind === 'deletedPart' && previousPiece.videoId === nextPiece.videoId) {
        return {
          videoId: previousPiece.videoId,
          piece: previousPiece,
          sourceSecond: previousPiece.sourceEndSecond + (second - previousPiece.timelineEndSecond),
          isInside: false,
          isDeleted: true,
        }
      }
      return { videoId: nextPiece.videoId, piece: nextPiece, sourceSecond: nextPiece.sourceStartSecond, isInside: false, isDeleted: false }
    }
    case 'before':
      return {
        videoId: position.nextPiece.videoId,
        piece: position.nextPiece,
        sourceSecond: position.nextPiece.sourceStartSecond,
        isInside: false,
        isDeleted: false,
      }
    case 'after': {
      const piece = position.previousPiece
      return { videoId: piece.videoId, piece, sourceSecond: framesToSeconds(piece.sourceEndFrame - 1, piece.fps), isInside: false, isDeleted: false }
    }
    default:
      return null
  }
}

/**
 * Returns the piece that plays after a piece ends, or null after the last piece.
 * @param {{pieces: Array<object>}} program Built program.
 * @param {string} pieceId Ending piece.
 * @returns {object|null} Next piece.
 */
export function findNextPiece(program, pieceId) {
  const index = program.pieces.findIndex((piece) => piece.id === pieceId)
  if (index === -1) throw new Error(`Unknown program piece: ${pieceId}`)
  return program.pieces[index + 1] ?? null
}

/**
 * Resolves where playback starts and which clock drives it. Inside a piece the
 * video plays; inside a gap or deleted part playback skips to the next piece.
 * Before the first and after the last piece the timeline clock plays the activity.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @param {number} second Requested timeline second.
 * @returns {{source: 'video'|'timeline', second: number}} Playback start.
 */
export function resolvePlaybackStart(program, second) {
  const position = resolveProgramPosition(program, second)
  switch (position.kind) {
    case 'piece':
    case 'transition':
      return { source: 'video', second }
    case 'gap':
      return { source: 'video', second: position.nextPiece.timelineStartSecond }
    default:
      return { source: 'timeline', second }
  }
}

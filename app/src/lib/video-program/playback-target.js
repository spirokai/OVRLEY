/**
 * Resolves which imported video and source frame the preview shows at a timeline second.
 */

import { framesToSeconds } from './frame-grid'
import { resolveProgramPosition } from './program'

/**
 * Inside a piece (including its outgoing transition window) the piece's own frame
 * is shown. Before the first piece and inside gaps the upcoming piece's first frame
 * is shown; after the last piece its final frame is shown.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @param {number} second Timeline second.
 * @returns {{videoId: string, sourceSecond: number, isInside: boolean}|null} Preview target, or null without pieces.
 */
export function resolvePreviewTarget(program, second) {
  const position = resolveProgramPosition(program, second)
  switch (position.kind) {
    case 'piece':
    case 'transition':
      return { videoId: position.piece.videoId, sourceSecond: position.sourceSecond, isInside: true }
    case 'before':
    case 'gap':
      return { videoId: position.nextPiece.videoId, sourceSecond: position.nextPiece.sourceStartSecond, isInside: false }
    case 'after': {
      const piece = position.previousPiece
      return { videoId: piece.videoId, sourceSecond: framesToSeconds(piece.sourceEndFrame - 1, piece.fps), isInside: false }
    }
    default:
      return null
  }
}

/**
 * Resolves which imported video and source frame the preview shows at a timeline
 * second, and where playback continues.
 */

import { framesToSeconds } from './frame-grid'
import { resolveProgramPosition } from './program'

function excludedTarget(program, second) {
  const segment = program.excludedSegments.find((candidate) => candidate.timelineStartSecond <= second && second < candidate.timelineEndSecond)
  if (segment === undefined) return null
  return {
    videoId: segment.videoId,
    piece: segment,
    sourceSecond: segment.sourceStartSecond + (second - segment.timelineStartSecond),
    isInside: false,
    isExcluded: true,
  }
}

/**
 * Inside a piece (including its outgoing transition window) the piece's own frame
 * is shown. Inside an excluded segment that no piece covers, the segment's frame is
 * shown and flagged. Before the first piece and inside gaps the upcoming piece's
 * first frame is shown; after the last piece its final frame is shown.
 * @param {{pieces: Array<object>, joins: Array<object>, excludedSegments: Array<object>}} program Built program.
 * @param {number} second Timeline second.
 * @returns {{videoId: string, piece: object, sourceSecond: number, isInside: boolean, isExcluded: boolean}|null}
 *   Preview target (`piece` is the piece or excluded segment whose video is shown), or null with nothing to show.
 */
export function resolvePreviewTarget(program, second) {
  const position = resolveProgramPosition(program, second)
  if (position.kind === 'piece' || position.kind === 'transition') {
    return { videoId: position.piece.videoId, piece: position.piece, sourceSecond: position.sourceSecond, isInside: true, isExcluded: false }
  }
  const excluded = excludedTarget(program, second)
  if (excluded !== null) return excluded
  switch (position.kind) {
    case 'gap':
    case 'before':
      return {
        videoId: position.nextPiece.videoId,
        piece: position.nextPiece,
        sourceSecond: position.nextPiece.sourceStartSecond,
        isInside: false,
        isExcluded: false,
      }
    case 'after': {
      const piece = position.previousPiece
      return { videoId: piece.videoId, piece, sourceSecond: framesToSeconds(piece.sourceEndFrame - 1, piece.fps), isInside: false, isExcluded: false }
    }
    default:
      return null
  }
}

/**
 * Resolves the clock that plays a piece: the video, or the timeline for a piece
 * of a missing video, which has no file to play.
 * @param {{missing: boolean}} piece Program piece.
 * @returns {'video'|'timeline'} Playback clock.
 */
export function pieceClock(piece) {
  return piece.missing ? 'timeline' : 'video'
}

/**
 * Resolves where playback starts and which clock drives it. Inside a piece the
 * video plays (the timeline clock plays a missing video's piece); inside a gap or an
 * excluded segment between pieces playback skips to the next piece. Before the
 * first and after the last piece the timeline clock plays the activity.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @param {number} second Requested timeline second.
 * @returns {{source: 'video'|'timeline', second: number}} Playback start.
 */
export function resolvePlaybackStart(program, second) {
  const position = resolveProgramPosition(program, second)
  switch (position.kind) {
    case 'piece':
    case 'transition':
      return { source: pieceClock(position.piece), second }
    case 'gap':
      return { source: pieceClock(position.nextPiece), second: position.nextPiece.timelineStartSecond }
    default:
      return { source: 'timeline', second }
  }
}

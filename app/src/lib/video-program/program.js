/**
 * Derives the output program (ordered pieces and the joins between them) from
 * the imported-video collection.
 *
 * A piece is a continuous source range of one video: a segment that is not
 * excluded from export, inside the export range. Overlapping pieces are resolved
 * for playback: the earlier-starting video plays to its end and the later video
 * continues at the matching source position. Overlaps are still reported so
 * single-file export can be blocked. Excluded segments are listed as well: they
 * stay on the timeline and in the preview, but leave no piece. A separate-file
 * export builds one program per video, so overlaps with other videos do not
 * trim it.
 *
 * A join connects consecutive pieces and belongs to the incoming piece's segment:
 * its `transitionIn` is the join's transition. A stored transition of `null` means
 * the join follows the project transition defaults, snapped to the frames of the
 * pieces it joins. Where the default does not fit, the join is a hard cut and its
 * `skippedDefault` is `{side, durationSeconds, roomSeconds}`: the piece it does not
 * fit (`previous` or `next`) and the seconds that piece has room for; otherwise null.
 */

import { framesToSeconds, secondsToFrames, snapDurationToFrames } from './frame-grid'
import { HARD_CUT, NO_TRANSITION } from './transition-catalog'
import { sortVideosByTimeline } from './video-collection'
import { listVideoSegments, segmentJoinKey } from './video-segments'

const TIME_EPSILON_SECONDS = 1e-6

function exportWindowFrames(video, exportRange) {
  if (exportRange.type !== 'custom') return { startFrame: 0, endFrame: video.frameCount }
  return {
    startFrame: secondsToFrames(exportRange.from - video.offsetSeconds, video.fps),
    endFrame: secondsToFrames(exportRange.to - video.offsetSeconds, video.fps),
  }
}

function createPiece(video, startFrame, endFrame, segment) {
  const sourceStartSecond = framesToSeconds(startFrame, video.fps)
  const sourceEndSecond = framesToSeconds(endFrame, video.fps)
  return {
    id: `${video.id}:${startFrame}`,
    videoId: video.id,
    // A piece of a missing video keeps its place; the preview shows that the file was not found.
    missing: video.missing,
    importOrder: video.importOrder,
    fps: video.fps,
    sourceStartFrame: startFrame,
    sourceEndFrame: endFrame,
    sourceStartSecond,
    sourceEndSecond,
    timelineStartSecond: video.offsetSeconds + sourceStartSecond,
    timelineEndSecond: video.offsetSeconds + sourceEndSecond,
    durationSeconds: sourceEndSecond - sourceStartSecond,
    segmentKey: segment.key,
    skipOverlay: segment.skipOverlay,
  }
}

function collectPieces(videos, exportRange) {
  const pieces = []
  for (const video of videos) {
    const window = exportWindowFrames(video, exportRange)
    for (const segment of listVideoSegments(video)) {
      const startFrame = Math.max(segment.startFrame, window.startFrame)
      const endFrame = Math.min(segment.endFrame, window.endFrame)
      if (!segment.excluded && endFrame > startFrame) pieces.push(createPiece(video, startFrame, endFrame, segment))
    }
  }
  return pieces.sort(
    (left, right) =>
      left.timelineStartSecond - right.timelineStartSecond || left.importOrder - right.importOrder || left.sourceStartFrame - right.sourceStartFrame,
  )
}

function collectExcludedSegments(videos) {
  return videos.flatMap((video) =>
    listVideoSegments(video)
      .filter((segment) => segment.excluded)
      .map((segment) => createPiece(video, segment.startFrame, segment.endFrame, segment)),
  )
}

function resolveOverlaps(sortedPieces, videosById) {
  const pieces = []
  const overlaps = []
  let runningEndSecond = Number.NEGATIVE_INFINITY
  let runningVideoId = null
  for (const piece of sortedPieces) {
    const overlapSeconds = runningEndSecond - piece.timelineStartSecond
    // Less than half a frame is no overlap: not one frame is shown twice.
    if (overlapSeconds < framesToSeconds(1, piece.fps) / 2) {
      pieces.push(piece)
      runningEndSecond = piece.timelineEndSecond
      runningVideoId = piece.videoId
      continue
    }

    overlaps.push({
      earlierVideoId: runningVideoId,
      laterVideoId: piece.videoId,
      startSecond: piece.timelineStartSecond,
      endSecond: Math.min(runningEndSecond, piece.timelineEndSecond),
    })
    const trimmedStartFrame = piece.sourceStartFrame + Math.ceil((overlapSeconds * piece.fps.num) / piece.fps.den - TIME_EPSILON_SECONDS)
    if (trimmedStartFrame < piece.sourceEndFrame) {
      pieces.push(
        createPiece(videosById.get(piece.videoId), trimmedStartFrame, piece.sourceEndFrame, {
          key: piece.segmentKey,
          skipOverlay: piece.skipOverlay,
        }),
      )
    }
    if (piece.timelineEndSecond > runningEndSecond) {
      runningEndSecond = piece.timelineEndSecond
      runningVideoId = piece.videoId
    }
  }
  return { pieces, overlaps }
}

/** Whether a piece keeps at least one frame outside its incoming and outgoing transitions. */
function leavesUntouchedFrame(piece, incomingSeconds, outgoingSeconds) {
  return incomingSeconds + outgoingSeconds + framesToSeconds(1, piece.fps) <= piece.durationSeconds + TIME_EPSILON_SECONDS
}

function transitionOverride(piece, videosById) {
  return videosById.get(piece.videoId).segments[piece.segmentKey].transitionIn
}

/** Seconds a piece leaves for a transition at one end, keeping one untouched frame beside the transition at its other end. */
function roomSeconds(piece, otherEndSeconds) {
  return piece.durationSeconds - otherEndSeconds - framesToSeconds(1, piece.fps)
}

/**
 * Resolves the transition of a join. A stored override wins. Otherwise separate videos closer
 * together than the continuity threshold (a camera's split recording) are joined with a hard
 * cut, and every other join gets the project default where it fits both pieces beside the
 * transitions they already have: the previous piece's resolved incoming join and the next
 * piece's outgoing override. Earlier defaults win over later ones.
 */
function resolveJoinTransition({ override, previous, next, gapSeconds, previousIncomingSeconds, nextOutgoingSeconds, transitionDefaults }) {
  if (override !== null) return { transition: override, skippedDefault: null }
  const isCameraSplit = previous.videoId !== next.videoId && gapSeconds <= transitionDefaults.splitGapThresholdSeconds + TIME_EPSILON_SECONDS
  if (isCameraSplit || transitionDefaults.type === NO_TRANSITION) return { transition: HARD_CUT, skippedDefault: null }

  const durationSeconds = snapDurationToFrames(transitionDefaults.durationSeconds, previous.fps)
  const sides = [
    { side: 'previous', roomSeconds: roomSeconds(previous, previousIncomingSeconds) },
    { side: 'next', roomSeconds: roomSeconds(next, nextOutgoingSeconds) },
  ]
  const tooShort = sides.find((candidate) => durationSeconds > candidate.roomSeconds + TIME_EPSILON_SECONDS)
  if (tooShort !== undefined) return { transition: HARD_CUT, skippedDefault: { ...tooShort, durationSeconds } }
  return { transition: { type: transitionDefaults.type, durationSeconds }, skippedDefault: null }
}

function buildJoins(pieces, videosById, transitionDefaults) {
  const joins = []
  for (let index = 1; index < pieces.length; index += 1) {
    const previous = pieces[index - 1]
    const next = pieces[index]
    const following = pieces[index + 1]
    const override = transitionOverride(next, videosById)
    const gapSeconds = Math.max(0, next.timelineStartSecond - previous.timelineEndSecond)
    const { transition, skippedDefault } = resolveJoinTransition({
      override,
      previous,
      next,
      gapSeconds,
      previousIncomingSeconds: joins.at(-1)?.transition.durationSeconds ?? 0,
      nextOutgoingSeconds: following === undefined ? 0 : (transitionOverride(following, videosById)?.durationSeconds ?? 0),
      transitionDefaults,
    })
    joins.push({
      key: segmentJoinKey(next.videoId, next.segmentKey),
      videoId: next.videoId,
      segmentKey: next.segmentKey,
      fromPieceId: previous.id,
      toPieceId: next.id,
      gapSeconds,
      transition,
      isDefault: override === null,
      skippedDefault,
    })
  }
  return joins
}

/**
 * Builds the output program for the imported-video collection.
 * @param {object} options Program inputs.
 * @param {Array<object>} options.videos Imported videos.
 * @param {{type: 'all'|'custom', from: number, to: number}} options.exportRange Export range in timeline seconds.
 * @param {{type: string, durationSeconds: number, splitGapThresholdSeconds: number}} options.transitionDefaults Project transition defaults.
 * @returns {{pieces: Array<object>, joins: Array<object>, overlaps: Array<object>, excludedSegments: Array<object>, durationSeconds: number}} Program in playback order.
 */
export function buildProgram({ videos, exportRange, transitionDefaults }) {
  const videosById = new Map(videos.map((video) => [video.id, video]))
  const { pieces, overlaps } = resolveOverlaps(collectPieces(sortVideosByTimeline(videos), exportRange), videosById)
  const joins = buildJoins(pieces, videosById, transitionDefaults)
  const piecesSeconds = pieces.reduce((total, piece) => total + piece.durationSeconds, 0)
  const transitionSeconds = joins.reduce((total, join) => total + join.transition.durationSeconds, 0)
  return { pieces, joins, overlaps, excludedSegments: collectExcludedSegments(videos), durationSeconds: piecesSeconds - transitionSeconds }
}

/**
 * Lists pieces whose incoming and outgoing transitions do not leave at least one
 * untouched frame.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @returns {Array<{pieceId: string, joinKeys: Array<string>}>} Invalid transition placements.
 */
export function findInvalidTransitions(program) {
  const issues = []
  program.pieces.forEach((piece, index) => {
    const incoming = program.joins[index - 1] ?? null
    const outgoing = program.joins[index] ?? null
    if (!leavesUntouchedFrame(piece, incoming?.transition.durationSeconds ?? 0, outgoing?.transition.durationSeconds ?? 0)) {
      issues.push({
        pieceId: piece.id,
        joinKeys: [incoming, outgoing].filter((join) => join && join.transition.durationSeconds > 0).map((join) => join.key),
      })
    }
  })
  return issues
}

/**
 * Resolves what the program shows at a timeline second.
 *
 * Inside the last `d` seconds of a piece whose outgoing join has a transition of
 * duration `d`, the result is a `transition` whose incoming source position
 * advances from the incoming piece start.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @param {number} second Timeline second.
 * @returns {object} `{kind: 'empty'}`, `{kind: 'before', nextPiece}`, `{kind: 'after', previousPiece}`,
 *   `{kind: 'gap', previousPiece, nextPiece, join}`, `{kind: 'piece', piece, sourceSecond, nextPiece, join}`, or
 *   `{kind: 'transition', piece, sourceSecond, nextPiece, join, progress, incomingSourceSecond}`.
 */
export function resolveProgramPosition(program, second) {
  const { pieces, joins } = program
  if (pieces.length === 0) return { kind: 'empty' }
  if (second < pieces[0].timelineStartSecond) return { kind: 'before', nextPiece: pieces[0] }

  for (let index = 0; index < pieces.length; index += 1) {
    const piece = pieces[index]
    const nextPiece = pieces[index + 1] ?? null
    const join = joins[index] ?? null
    if (second >= piece.timelineEndSecond) {
      if (nextPiece && second < nextPiece.timelineStartSecond) return { kind: 'gap', previousPiece: piece, nextPiece, join }
      continue
    }

    const sourceSecond = piece.sourceStartSecond + (second - piece.timelineStartSecond)
    const transitionSeconds = join?.transition.durationSeconds ?? 0
    const windowStartSecond = piece.timelineEndSecond - transitionSeconds
    if (transitionSeconds > 0 && second >= windowStartSecond) {
      const elapsedSeconds = second - windowStartSecond
      return {
        kind: 'transition',
        piece,
        sourceSecond,
        nextPiece,
        join,
        progress: elapsedSeconds / transitionSeconds,
        incomingSourceSecond: nextPiece.sourceStartSecond + elapsedSeconds,
      }
    }
    return { kind: 'piece', piece, sourceSecond, nextPiece, join }
  }

  return { kind: 'after', previousPiece: pieces.at(-1) }
}

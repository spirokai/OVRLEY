/**
 * Derives the output program (ordered pieces and the joins between them) from
 * the imported-video collection.
 *
 * A piece is a continuous source range of one video: a segment that is not
 * excluded from export, inside the export range. Overlapping pieces are resolved
 * for playback: the earlier-starting video plays to its end and the later video
 * continues at the matching source position. Overlaps are still reported so
 * single-file export can be blocked. Excluded segments are listed as well: they
 * stay on the timeline and in the preview, but leave no piece.
 *
 * A join connects consecutive pieces and belongs to the incoming piece's segment:
 * its `transitionIn` is the join's transition. A stored transition of `null` means
 * the join follows the project transition defaults, snapped to the frames of the
 * pieces it joins.
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

function createPiece(video, startFrame, endFrame, segmentKey) {
  const sourceStartSecond = framesToSeconds(startFrame, video.fps)
  const sourceEndSecond = framesToSeconds(endFrame, video.fps)
  return {
    id: `${video.id}:${startFrame}`,
    videoId: video.id,
    importOrder: video.importOrder,
    fps: video.fps,
    sourceStartFrame: startFrame,
    sourceEndFrame: endFrame,
    sourceStartSecond,
    sourceEndSecond,
    timelineStartSecond: video.offsetSeconds + sourceStartSecond,
    timelineEndSecond: video.offsetSeconds + sourceEndSecond,
    durationSeconds: sourceEndSecond - sourceStartSecond,
    segmentKey,
  }
}

function collectPieces(videos, exportRange) {
  const pieces = []
  for (const video of videos) {
    const window = exportWindowFrames(video, exportRange)
    for (const segment of listVideoSegments(video)) {
      const startFrame = Math.max(segment.startFrame, window.startFrame)
      const endFrame = Math.min(segment.endFrame, window.endFrame)
      if (!segment.excluded && endFrame > startFrame) pieces.push(createPiece(video, startFrame, endFrame, segment.key))
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
      .map((segment) => createPiece(video, segment.startFrame, segment.endFrame, segment.key)),
  )
}

/**
 * Lists the pieces each video keeps after its excluded segments and the export
 * range, without resolving overlaps between videos. Separate-file export renders
 * these; the combined program trims overlapping pieces (see `buildProgram`).
 * @param {object} options Piece inputs.
 * @param {Array<object>} options.videos Imported videos joined with their edits.
 * @param {{type: 'all'|'custom', from: number, to: number}} options.exportRange Export range in timeline seconds.
 * @returns {Map<string, Array<object>>} Pieces of each video that keeps any, in source order.
 */
export function collectVideoPieces({ videos, exportRange }) {
  const piecesByVideo = new Map()
  for (const piece of collectPieces(videos, exportRange)) {
    piecesByVideo.set(piece.videoId, [...(piecesByVideo.get(piece.videoId) ?? []), piece])
  }
  return piecesByVideo
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
      pieces.push(createPiece(videosById.get(piece.videoId), trimmedStartFrame, piece.sourceEndFrame, piece.segmentKey))
    }
    if (piece.timelineEndSecond > runningEndSecond) {
      runningEndSecond = piece.timelineEndSecond
      runningVideoId = piece.videoId
    }
  }
  return { pieces, overlaps }
}

function defaultTransition(previous, next, gapSeconds, transitionDefaults) {
  if (gapSeconds <= transitionDefaults.splitGapThresholdSeconds + TIME_EPSILON_SECONDS) return HARD_CUT
  if (transitionDefaults.type === NO_TRANSITION) return HARD_CUT
  const durationSeconds = snapDurationToFrames(transitionDefaults.durationSeconds, previous.fps)
  const fits = (piece) => 2 * durationSeconds + framesToSeconds(1, piece.fps) <= piece.durationSeconds + TIME_EPSILON_SECONDS
  if (!fits(previous) || !fits(next)) return HARD_CUT
  return { type: transitionDefaults.type, durationSeconds }
}

function buildJoins(pieces, videosById, transitionDefaults) {
  return pieces.slice(1).map((next, index) => {
    const previous = pieces[index]
    const override = videosById.get(next.videoId).segments[next.segmentKey].transitionIn
    const gapSeconds = Math.max(0, next.timelineStartSecond - previous.timelineEndSecond)
    return {
      key: segmentJoinKey(next.videoId, next.segmentKey),
      videoId: next.videoId,
      segmentKey: next.segmentKey,
      fromPieceId: previous.id,
      toPieceId: next.id,
      gapSeconds,
      transition: override ?? defaultTransition(previous, next, gapSeconds, transitionDefaults),
      isDefault: override === null,
    }
  })
}

/**
 * Lists the transitions between consecutive pieces of one video (its separate
 * output), by the same rule as the program.
 * @param {object} video Imported video joined with its edits.
 * @param {Array<object>} pieces The video's pieces in source order (`collectVideoPieces`).
 * @param {{type: string, durationSeconds: number, splitGapThresholdSeconds: number}} transitionDefaults Project transition defaults.
 * @returns {Array<{type: string, durationSeconds: number}>} One transition per join.
 */
export function buildVideoPieceTransitions(video, pieces, transitionDefaults) {
  return pieces.slice(1).map((next, index) => {
    const previous = pieces[index]
    const gapSeconds = next.timelineStartSecond - previous.timelineEndSecond
    return video.segments[next.segmentKey].transitionIn ?? defaultTransition(previous, next, gapSeconds, transitionDefaults)
  })
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
    const required = (incoming?.transition.durationSeconds ?? 0) + (outgoing?.transition.durationSeconds ?? 0) + framesToSeconds(1, piece.fps)
    if (required > piece.durationSeconds + TIME_EPSILON_SECONDS) {
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

/**
 * Derives the output program (ordered pieces and the joins between them) from
 * the imported-video collection.
 *
 * A piece is a continuous source range of one video that survives deleted parts
 * and the export range. Overlapping pieces are resolved for playback: the
 * earlier-starting video plays to its end and the later video continues at the
 * matching source position. Overlaps are still reported so single-file export
 * can be blocked.
 *
 * A join connects consecutive pieces. It is a `clip` join when the incoming
 * piece is the first piece of its video (transition stored on
 * `video.transitionIn`) and a `deletedPart` join otherwise (transition stored on
 * the deleted part that precedes the incoming piece). A stored transition of
 * `null` means the join follows the project transition defaults.
 */

import { framesToSeconds, secondsToFrames } from './frame-grid'
import { HARD_CUT, NO_TRANSITION } from './transition-catalog'
import { sortVideosByTimeline } from './video-collection'

const TIME_EPSILON_SECONDS = 1e-6

function keptSegments(video) {
  const parts = video.deletedParts
    .map((part) => ({ id: part.id, startFrame: secondsToFrames(part.fromSecond, video.fps), endFrame: secondsToFrames(part.toSecond, video.fps) }))
    .sort((left, right) => left.startFrame - right.startFrame)
  const segments = []
  let cursor = 0
  let leadingDeletedPartId = null
  for (const part of parts) {
    if (part.startFrame > cursor) segments.push({ startFrame: cursor, endFrame: part.startFrame, leadingDeletedPartId })
    cursor = Math.max(cursor, part.endFrame)
    leadingDeletedPartId = segments.length > 0 ? part.id : null
  }
  if (video.frameCount > cursor) segments.push({ startFrame: cursor, endFrame: video.frameCount, leadingDeletedPartId })
  return segments
}

function exportWindowFrames(video, exportRange) {
  if (exportRange.type !== 'custom') return { startFrame: 0, endFrame: video.frameCount }
  return {
    startFrame: secondsToFrames(exportRange.from - video.offsetSeconds, video.fps),
    endFrame: secondsToFrames(exportRange.to - video.offsetSeconds, video.fps),
  }
}

function createPiece(video, startFrame, endFrame, leadingDeletedPartId) {
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
    leadingDeletedPartId,
  }
}

function collectPieces(videos, exportRange) {
  const pieces = []
  for (const video of videos) {
    const window = exportWindowFrames(video, exportRange)
    for (const segment of keptSegments(video)) {
      const startFrame = Math.max(segment.startFrame, window.startFrame)
      const endFrame = Math.min(segment.endFrame, window.endFrame)
      if (endFrame > startFrame) pieces.push(createPiece(video, startFrame, endFrame, segment.leadingDeletedPartId))
    }
  }
  return pieces.sort(
    (left, right) =>
      left.timelineStartSecond - right.timelineStartSecond || left.importOrder - right.importOrder || left.sourceStartFrame - right.sourceStartFrame,
  )
}

function resolveOverlaps(sortedPieces, videosById) {
  const pieces = []
  const overlaps = []
  let runningEndSecond = Number.NEGATIVE_INFINITY
  let runningVideoId = null
  for (const piece of sortedPieces) {
    const overlapSeconds = runningEndSecond - piece.timelineStartSecond
    if (overlapSeconds <= TIME_EPSILON_SECONDS) {
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
      pieces.push(createPiece(videosById.get(piece.videoId), trimmedStartFrame, piece.sourceEndFrame, piece.leadingDeletedPartId))
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
  const { durationSeconds } = transitionDefaults
  const fits = (piece) => 2 * durationSeconds + framesToSeconds(1, piece.fps) <= piece.durationSeconds + TIME_EPSILON_SECONDS
  if (!fits(previous) || !fits(next)) return HARD_CUT
  return { type: transitionDefaults.type, durationSeconds }
}

function buildJoins(pieces, videosById, transitionDefaults) {
  const deletedPartsById = new Map()
  for (const video of videosById.values()) {
    for (const part of video.deletedParts) deletedPartsById.set(part.id, part)
  }

  const seenVideoIds = new Set(pieces.length > 0 ? [pieces[0].videoId] : [])
  const joins = []
  for (let index = 1; index < pieces.length; index += 1) {
    const previous = pieces[index - 1]
    const next = pieces[index]
    const kind = seenVideoIds.has(next.videoId) ? 'deletedPart' : 'clip'
    seenVideoIds.add(next.videoId)
    const ownerId = kind === 'clip' ? next.videoId : next.leadingDeletedPartId
    const override = kind === 'clip' ? videosById.get(ownerId).transitionIn : deletedPartsById.get(ownerId).transition
    const gapSeconds = Math.max(0, next.timelineStartSecond - previous.timelineEndSecond)
    joins.push({
      key: `${kind}:${ownerId}`,
      kind,
      ownerId,
      fromPieceId: previous.id,
      toPieceId: next.id,
      gapSeconds,
      transition: override ?? defaultTransition(previous, next, gapSeconds, transitionDefaults),
      isDefault: override === null,
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
 * @returns {{pieces: Array<object>, joins: Array<object>, overlaps: Array<object>, durationSeconds: number}} Program in playback order.
 */
export function buildProgram({ videos, exportRange, transitionDefaults }) {
  const videosById = new Map(videos.map((video) => [video.id, video]))
  const { pieces, overlaps } = resolveOverlaps(collectPieces(sortVideosByTimeline(videos), exportRange), videosById)
  const joins = buildJoins(pieces, videosById, transitionDefaults)
  const piecesSeconds = pieces.reduce((total, piece) => total + piece.durationSeconds, 0)
  const transitionSeconds = joins.reduce((total, join) => total + join.transition.durationSeconds, 0)
  return { pieces, joins, overlaps, durationSeconds: piecesSeconds - transitionSeconds }
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

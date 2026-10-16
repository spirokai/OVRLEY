/**
 * Transition windows of the program and the preview proxies that show them.
 *
 * A join with a transition of duration `d` blends the last `d` seconds of the
 * outgoing piece into the first `d` seconds of the incoming one. On the editor
 * timeline the window is the end of the outgoing piece; afterwards playback
 * continues `d` seconds into the incoming piece. A proxy is a short native
 * render of that blend, keyed by everything that affects it.
 */

import { NO_TRANSITION } from './transition-catalog'
import { resolveProgramPosition } from './program'

/** Proxies are rendered at most this tall (and never taller than the outgoing video). */
export const PROXY_MAX_HEIGHT = 360

function evenRound(value) {
  return Math.max(2, 2 * Math.round(value / 2))
}

function proxySize(resolution) {
  const height = evenRound(Math.min(PROXY_MAX_HEIGHT, resolution.height))
  return { width: evenRound((height * resolution.width) / resolution.height), height }
}

/**
 * Resolves the transition window at a timeline second.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @param {number} second Timeline second.
 * @returns {{join: object, outgoingPiece: object, incomingPiece: object, progress: number, elapsedSeconds: number, incomingSecond: number}|null}
 *   The window (`incomingSecond` is the timeline second the incoming overlay shows), or null outside every window.
 */
export function resolveTransitionWindow(program, second) {
  const position = resolveProgramPosition(program, second)
  if (position.kind !== 'transition') return null
  const elapsedSeconds = position.progress * position.join.transition.durationSeconds
  return {
    join: position.join,
    outgoingPiece: position.piece,
    incomingPiece: position.nextPiece,
    progress: position.progress,
    elapsedSeconds,
    incomingSecond: position.nextPiece.timelineStartSecond + elapsedSeconds,
  }
}

/**
 * Timeline second where playback continues after a piece: the incoming piece's
 * start, plus the transition that already showed its first seconds.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @param {string} pieceId Ending piece.
 * @returns {number|null} Timeline second, or null after the last piece.
 */
export function resolveContinuationSecond(program, pieceId) {
  const index = program.pieces.findIndex((piece) => piece.id === pieceId)
  if (index === -1) throw new Error(`Unknown program piece: ${pieceId}`)
  const next = program.pieces[index + 1]
  if (next === undefined) return null
  return next.timelineStartSecond + program.joins[index].transition.durationSeconds
}

/**
 * Lists the proxies the program needs: one per join with an `xfade` transition.
 * @param {{pieces: Array<object>, joins: Array<object>}} program Built program.
 * @param {Array<object>} clips Timeline clips of the program.
 * @returns {Array<{key: string, joinKey: string, windowStartSecond: number, request: object}>} Proxy jobs; `key` identifies the content.
 */
export function listTransitionProxyJobs(program, clips) {
  const piecesById = new Map(program.pieces.map((piece) => [piece.id, piece]))
  const clipsById = new Map(clips.map((clip) => [clip.id, clip]))
  return program.joins
    .filter((join) => join.transition.type !== NO_TRANSITION)
    .map((join) => {
      const outgoingPiece = piecesById.get(join.fromPieceId)
      const incomingPiece = piecesById.get(join.toPieceId)
      const outgoing = clipsById.get(outgoingPiece.videoId)
      const incoming = clipsById.get(incomingPiece.videoId)
      const { durationSeconds, type } = join.transition
      const request = {
        outgoing: { path: outgoing.path, startSecond: outgoingPiece.sourceEndSecond - durationSeconds, hasAudio: outgoing.hasAudio },
        incoming: { path: incoming.path, startSecond: incomingPiece.sourceStartSecond, hasAudio: incoming.hasAudio },
        transition: type,
        durationSeconds,
        ...proxySize(outgoing.resolution),
        fpsNum: outgoing.fps.num,
        fpsDen: outgoing.fps.den,
      }
      return {
        key: JSON.stringify(request),
        joinKey: join.key,
        windowStartSecond: outgoingPiece.timelineEndSecond - durationSeconds,
        request,
      }
    })
}

/**
 * Picks the next proxy to render: the missing one whose window is nearest the playhead.
 * @param {Array<object>} jobs Proxy jobs (`listTransitionProxyJobs`).
 * @param {Object<string, object>} proxies Known proxies keyed by content key.
 * @param {number} second Playhead second.
 * @returns {object|null} Next job, or null when every proxy is known.
 */
export function pickNextTransitionProxyJob(jobs, proxies, second) {
  const missing = jobs.filter((job) => proxies[job.key] === undefined)
  if (missing.length === 0) return null
  const distance = (job) => Math.max(0, job.windowStartSecond - second, second - (job.windowStartSecond + job.request.durationSeconds))
  return missing.reduce((nearest, job) => (distance(job) < distance(nearest) ? job : nearest))
}

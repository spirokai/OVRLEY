import {
  VIDEO_SYNC_CANDIDATE_MERGE_TOLERANCE_SECONDS,
  VIDEO_SYNC_LANDMARK_TYPES,
  VIDEO_SYNC_MAX_CANDIDATES,
  VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
} from '../data/videoSyncConstants'
import { calculateMatchScore, compareCandidateStrength } from './matchScore'
import { createVideoSyncOffsetSupports, MATCHABLE_LANDMARK_TYPES, offsetIsSupported, serializeOffsetSupport } from './landmarkTiming'

/**
 * Sweeps support boundaries and returns regions supported by at least two landmarks.
 *
 * @param {{landmarkId: string, startOffset: number, endOffset: number}[]} supports Typed offset supports.
 * @returns {{start: number, end: number, supports: object[]}[]} Supported offset regions.
 */
function findConsensusRegions(supports) {
  const boundaries = new Map()

  /**
   * Adds a support to a start or end boundary bucket.
   *
   * @param {number} value Offset boundary.
   * @param {'starts'|'ends'} kind Boundary side.
   * @param {object} support Typed offset support.
   * @returns {void}
   */
  const add = (value, kind, support) => {
    if (!boundaries.has(value)) boundaries.set(value, { starts: [], ends: [] })
    boundaries.get(value)[kind].push(support)
  }
  for (const support of supports) {
    add(support.startOffset, 'starts', support)
    add(support.endOffset, 'ends', support)
  }

  const points = [...boundaries.keys()].sort((left, right) => left - right)
  const active = new Set()
  const activeLandmarks = new Map()
  const regions = []

  /**
   * Activates one support and updates its landmark count.
   *
   * @param {object} support Typed offset support.
   * @returns {void}
   */
  const addActive = (support) => {
    active.add(support)
    activeLandmarks.set(support.landmarkId, (activeLandmarks.get(support.landmarkId) ?? 0) + 1)
  }

  /**
   * Deactivates one support and updates its landmark count.
   *
   * @param {object} support Typed offset support.
   * @returns {void}
   */
  const removeActive = (support) => {
    active.delete(support)
    const count = activeLandmarks.get(support.landmarkId)
    if (count === 1) activeLandmarks.delete(support.landmarkId)
    else activeLandmarks.set(support.landmarkId, count - 1)
  }

  for (let index = 0; index < points.length; index += 1) {
    const start = points[index]
    const boundary = boundaries.get(start)
    for (const support of boundary.starts) addActive(support)
    if (activeLandmarks.size >= 2) regions.push({ start, end: start, supports: [...active] })

    for (const support of boundary.ends) removeActive(support)
    const end = points[index + 1]
    if (end !== undefined && activeLandmarks.size >= 2) regions.push({ start, end, supports: [...active] })
  }
  return regions
}

/**
 * Compares two DP states with the same match count.
 *
 * @param {{residuals: number[], matches: object[]} } left First state.
 * @param {{residuals: number[], matches: object[]} } right Second state.
 * @returns {number} Negative when the first state is stronger.
 */
function stateStrength(left, right) {
  /**
   * Sums squared timing residuals for a DP state.
   *
   * @param {{residuals: number[]}} state DP state.
   * @returns {number} Squared residual total.
   */
  const error = (state) => state.residuals.reduce((sum, residual) => sum + residual ** 2, 0)

  /**
   * Builds a stable assignment tie-break key.
   *
   * @param {{matches: {support: {landmarkId: string, eventId: string}}[]}} state DP state.
   * @returns {string} Stable assignment key.
   */
  const key = (state) => state.matches.map(({ support }) => `${support.landmarkId}:${support.eventId}`).join('|')
  return error(left) - error(right) || key(left).localeCompare(key(right))
}

/**
 * Keeps the stronger state for one match count in a DP cell.
 *
 * @param {Map<number, object>} states DP cell states by match count.
 * @param {number} count Number of matches.
 * @param {object} state Candidate state.
 * @returns {void}
 */
function keepState(states, count, state) {
  if (!states.has(count) || stateStrength(state, states.get(count)) < 0) states.set(count, state)
}

/**
 * Assigns ordered events to ordered landmarks of one canonical type.
 *
 * @param {{landmarks: object[], events: object[], supportByLandmark: Map<string, Map<string, object>>}} input Prepared type-specific assignment data.
 * @param {number} offset Candidate video offset.
 * @returns {Map<number, {residuals: number[], matches: object[]}>} Best states by match count.
 */
function assignType({ landmarks, events, supportByLandmark }, offset) {
  const table = Array.from({ length: landmarks.length + 1 }, () => Array.from({ length: events.length + 1 }, () => new Map()))
  table[0][0].set(0, { residuals: [], matches: [] })
  for (let landmarkIndex = 0; landmarkIndex <= landmarks.length; landmarkIndex += 1) {
    for (let eventIndex = 0; eventIndex <= events.length; eventIndex += 1) {
      if (landmarkIndex === 0 && eventIndex === 0) continue
      const states = table[landmarkIndex][eventIndex]
      if (landmarkIndex > 0) {
        for (const [count, state] of table[landmarkIndex - 1][eventIndex]) keepState(states, count, state)
      }
      if (eventIndex > 0) {
        for (const [count, state] of table[landmarkIndex][eventIndex - 1]) keepState(states, count, state)
      }
      if (landmarkIndex === 0 || eventIndex === 0) continue

      const landmark = landmarks[landmarkIndex - 1]
      const event = events[eventIndex - 1]
      const support = supportByLandmark.get(landmark.id)?.get(event.id)
      if (support === undefined || !offsetIsSupported(support, offset)) continue
      const residual = support.residualAt(offset)
      for (const [count, state] of table[landmarkIndex - 1][eventIndex - 1]) {
        keepState(states, count + 1, {
          residuals: [...state.residuals, residual],
          matches: [...state.matches, { support, residual }],
        })
      }
    }
  }
  return table[landmarks.length][events.length]
}

/**
 * Prepares the ordered landmark, event, and support lookup data reused by every region.
 *
 * @param {object[]} eligibleLandmarks Landmarks eligible for ordinary matching.
 * @param {object[]} supports Typed offset supports.
 * @returns {{landmarks: object[], events: object[], supportByLandmark: Map<string, Map<string, object>>}[]} Prepared data for each matchable type.
 */
function prepareAssignmentInputs(eligibleLandmarks, supports) {
  return MATCHABLE_LANDMARK_TYPES.map((type) => {
    const landmarks = eligibleLandmarks.filter((landmark) => landmark.type === type)
    const supportByLandmark = new Map()
    const eventsById = new Map()

    for (const support of supports) {
      if (support.type !== type) continue
      if (!supportByLandmark.has(support.landmarkId)) supportByLandmark.set(support.landmarkId, new Map())
      supportByLandmark.get(support.landmarkId).set(support.eventId, support)
      eventsById.set(support.eventId, support.event)
    }

    const events = [...eventsById.values()].sort(
      (left, right) =>
        (left.type === VIDEO_SYNC_LANDMARK_TYPES.STOP ? left.time : left.start) -
          (right.type === VIDEO_SYNC_LANDMARK_TYPES.STOP ? right.time : right.start) || left.id.localeCompare(right.id),
    )
    return { landmarks, events, supportByLandmark }
  })
}

/**
 * Finds the strongest one-to-one assignment at one offset.
 *
 * @param {number} offset Candidate video offset.
 * @param {{landmarks: object[], events: object[], supportByLandmark: Map<string, Map<string, object>>}[]} assignmentInputs Prepared type-specific assignment data.
 * @param {number} eligibleCount Number of eligible landmarks.
 * @returns {{residuals: number[], matches: object[], score: object}|null} Strongest qualifying assignment.
 */
function bestAssignment(offset, assignmentInputs, eligibleCount) {
  let assignments = new Map([[0, { residuals: [], matches: [] }]])
  for (const input of assignmentInputs) {
    const next = new Map()
    const typeAssignments = assignType(input, offset)
    for (const [leftCount, left] of assignments) {
      for (const [rightCount, right] of typeAssignments) {
        keepState(next, leftCount + rightCount, {
          residuals: [...left.residuals, ...right.residuals],
          matches: [...left.matches, ...right.matches],
        })
      }
    }
    assignments = next
  }

  if (eligibleCount === 2) {
    const exact = assignments.get(2)
    return exact === undefined ? null : { ...exact, score: calculateMatchScore(exact.residuals, 2) }
  }
  const candidates = [...assignments.entries()]
    .filter(([count]) => count >= 2)
    .map(([, assignment]) => ({ ...assignment, score: calculateMatchScore(assignment.residuals, eligibleCount) }))
  candidates.sort((left, right) => compareCandidateStrength({ ...left.score, offset }, { ...right.score, offset }))
  return candidates[0] ?? null
}

/**
 * Selects one refined offset per consensus region and reevaluates its assignment.
 *
 * @param {{start: number, end: number, supports: object[]}} region Consensus region.
 * @param {{landmarks: object[], events: object[], supportByLandmark: Map<string, Map<string, object>>}[]} assignmentInputs Prepared type-specific assignment data.
 * @param {number} eligibleCount Number of eligible landmarks.
 * @returns {{offset: number, assignment: object}|null} Refined qualifying assignment.
 */
function refineRegion(region, assignmentInputs, eligibleCount) {
  const midpoint = (region.start + region.end) / 2
  const initial = bestAssignment(midpoint, assignmentInputs, eligibleCount)
  if (initial === null) return null
  if (region.start === region.end) return { offset: midpoint, assignment: initial }

  const anchors = initial.matches
    .map(({ support }) => {
      if (support.type === VIDEO_SYNC_LANDMARK_TYPES.STOP) return support.preferredOffsets[0]
      const [start, end] = support.preferredOffsets
      return midpoint < start ? start : midpoint > end ? end : null
    })
    .filter((anchor) => anchor !== null)
  const offset =
    anchors.length === 0 ? midpoint : Math.max(region.start, Math.min(region.end, anchors.reduce((sum, value) => sum + value, 0) / anchors.length))
  const assignment = bestAssignment(offset, assignmentInputs, eligibleCount)
  if (assignment === null) return null
  return { offset, assignment }
}

/**
 * Builds an ordinary candidate record and retains its score for ranking and diagnostics.
 *
 * @param {{offset: number, assignment: {matches: {support: object, residual: number}[], score: object}}} refined Refined assignment.
 * @param {number} eligibleCount Number of eligible landmarks.
 * @returns {{data: object, score: object}} Internal candidate record.
 */
function createCandidate(refined, eligibleCount) {
  const evidence = refined.assignment.matches.map(({ support, residual }) => ({
    landmarkId: support.landmarkId,
    eventId: support.eventId,
    type: support.type,
    residualSeconds: residual,
  }))
  return {
    data: {
      variant: 'ordinary',
      offset: refined.offset,
      matchScore: refined.assignment.score.matchScore,
      matchedCount: evidence.length,
      eligibleCount,
      evidence,
      mapClassification: 'none',
    },
    score: refined.assignment.score,
  }
}

/**
 * Compares two internal ordinary candidate records.
 *
 * @param {{data: {offset: number}, score: object}} left First candidate.
 * @param {{data: {offset: number}, score: object}} right Second candidate.
 * @returns {number} Candidate ordering result.
 */
function compareCandidates(left, right) {
  return compareCandidateStrength({ ...left.score, offset: left.data.offset }, { ...right.score, offset: right.data.offset })
}

/**
 * Merges nearby ordinary candidates and orders the retained representatives.
 *
 * @param {{data: {offset: number}, score: object}[]} candidates Candidate records.
 * @returns {{data: object, score: object}[]} Merged candidates in strength order.
 */
function mergeCandidates(candidates) {
  const sorted = [...candidates].sort((left, right) => left.data.offset - right.data.offset)
  const merged = []
  for (const candidate of sorted) {
    const previous = merged.at(-1)
    if (previous !== undefined && candidate.data.offset - previous.data.offset <= VIDEO_SYNC_CANDIDATE_MERGE_TOLERANCE_SECONDS) {
      if (compareCandidates(candidate, previous) < 0) merged[merged.length - 1] = candidate
    } else {
      merged.push(candidate)
    }
  }
  return merged.sort(compareCandidates)
}

/**
 * Creates the unscored authoritative candidate for a resolved location landmark.
 *
 * @param {{id: string}} landmark Resolved location landmark.
 * @param {number} offset Map-derived offset.
 * @param {number} eligibleCount Number of ordinary eligible landmarks.
 * @returns {object} Map-only presentation candidate.
 */
function createMapOnlyCandidate(landmark, offset, eligibleCount) {
  return {
    variant: 'mapOnly',
    offset,
    matchScore: null,
    matchedCount: 0,
    eligibleCount,
    evidence: [],
    mapLandmarkId: landmark.id,
    mapOffset: offset,
    mapClassification: 'authoritative',
  }
}

/**
 * Finds deterministic, globally aligned manual video-sync candidates.
 *
 * @param {object[]} landmarks Canonical video landmarks.
 * @param {{availability: {speed: boolean, heading: boolean}, stops: object[], turns: object[]}} detection Canonical detected activity events.
 * @returns {{candidates: object[], diagnostics: {eligibleLandmarkIds: string[], offsetSupports: object[], hypotheses: object[]}}} Presentation candidates and development diagnostics.
 */
export function matchVideoSyncCandidates(landmarks, detection) {
  const { eligibleLandmarks, supports } = createVideoSyncOffsetSupports(landmarks, detection)
  const assignmentInputs = prepareAssignmentInputs(eligibleLandmarks, supports)
  const ordinary = []
  for (const region of findConsensusRegions(supports)) {
    const refined = refineRegion(region, assignmentInputs, eligibleLandmarks.length)
    if (refined !== null) ordinary.push(createCandidate(refined, eligibleLandmarks.length))
  }

  const merged = mergeCandidates(ordinary)
  const mapLandmark = landmarks.find((landmark) => landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION && landmark.activitySecond !== null)
  const mapOffset = mapLandmark === undefined ? null : mapLandmark.activitySecond - mapLandmark.videoSecond
  const classified = merged.map((record) => {
    if (mapLandmark === undefined) return record
    const agreesWithMap = Math.abs(record.data.offset - mapOffset) <= VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS
    return {
      ...record,
      data: {
        ...record.data,
        variant: agreesWithMap ? 'ordinary' : 'mapConflict',
        mapLandmarkId: mapLandmark.id,
        mapOffset,
        mapClassification: agreesWithMap ? 'agrees' : 'conflict',
        ...(agreesWithMap ? {} : { excludedLandmarkIds: [mapLandmark.id] }),
      },
    }
  })
  const ordinaryLimit = mapLandmark === undefined ? VIDEO_SYNC_MAX_CANDIDATES : VIDEO_SYNC_MAX_CANDIDATES - 1
  const candidates = classified.slice(0, ordinaryLimit).map((record) => record.data)
  if (mapLandmark !== undefined) candidates.unshift(createMapOnlyCandidate(mapLandmark, mapOffset, eligibleLandmarks.length))

  return {
    candidates,
    diagnostics: {
      eligibleLandmarkIds: eligibleLandmarks.map((landmark) => landmark.id),
      offsetSupports: supports.map(serializeOffsetSupport),
      hypotheses: merged.map((record) => ({
        offset: record.data.offset,
        ...record.score,
        evidence: record.data.evidence,
      })),
    },
  }
}

import {
  VIDEO_SYNC_CANDIDATE_MERGE_TOLERANCE_SECONDS,
  VIDEO_SYNC_LANDMARK_TYPES,
  VIDEO_SYNC_LOCATION_TIMING_TOLERANCE_SECONDS,
  VIDEO_SYNC_MATCH_SCOPES,
  VIDEO_SYNC_MAX_CANDIDATES,
} from '../data/videoSyncConstants'
import { calculateMatchScore, calculateTimingQuality, compareCandidateStrength, VIDEO_SYNC_MIN_MEANINGFUL_QUALITY } from './matchScore'
import { calculateOffsetResidual, createVideoSyncOffsetSupports, MATCHABLE_LANDMARK_TYPES } from './landmarkTiming'

const MAX_PROPOSAL_NORMALIZED_MISS = Math.sqrt(1 / VIDEO_SYNC_MIN_MEANINGFUL_QUALITY - 1)

function compareAssignments(candidate, current) {
  if (candidate === null) return 1
  if (current === null) return -1
  if (candidate.totalQuality !== current.totalQuality) return current.totalQuality - candidate.totalQuality
  if (candidate.totalResidualSeconds !== current.totalResidualSeconds) {
    return candidate.totalResidualSeconds - current.totalResidualSeconds
  }
  const candidateKey = candidate.matches.map(({ support }) => `${support.landmarkId}:${support.eventId}`).join('|')
  const currentKey = current.matches.map(({ support }) => `${support.landmarkId}:${support.eventId}`).join('|')
  return candidateKey.localeCompare(currentKey)
}

function keepStrongerAssignment(current, candidate) {
  return compareAssignments(candidate, current) < 0 ? candidate : current
}

function proposedOffset(left, right) {
  if (left.type === right.type && (left.eventId === right.eventId || left.eventSecond >= right.eventSecond)) return null

  const overlapStart = Math.max(left.startOffset, right.startOffset)
  const overlapEnd = Math.min(left.endOffset, right.endOffset)
  if (overlapStart <= overlapEnd) return (overlapStart + overlapEnd) / 2

  const earlier = left.endOffset < right.startOffset ? left : right
  const later = earlier === left ? right : left
  const gap = later.startOffset - earlier.endOffset
  const combinedScale = earlier.timingScaleSeconds + later.timingScaleSeconds
  if (gap > MAX_PROPOSAL_NORMALIZED_MISS * combinedScale) return null

  return (earlier.endOffset * later.timingScaleSeconds + later.startOffset * earlier.timingScaleSeconds) / combinedScale
}

function createOffsetHypotheses(landmarks, supports) {
  const supportsByLandmark = new Map(landmarks.map((landmark) => [landmark.id, []]))
  for (const support of supports) supportsByLandmark.get(support.landmarkId).push(support)

  const hypotheses = new Map()
  for (let leftIndex = 0; leftIndex < landmarks.length; leftIndex += 1) {
    const leftSupports = supportsByLandmark.get(landmarks[leftIndex].id)
    for (let rightIndex = leftIndex + 1; rightIndex < landmarks.length; rightIndex += 1) {
      const rightSupports = supportsByLandmark.get(landmarks[rightIndex].id)
      for (const left of leftSupports) {
        for (const right of rightSupports) {
          const maximumGap = MAX_PROPOSAL_NORMALIZED_MISS * (left.timingScaleSeconds + right.timingScaleSeconds)
          if (right.endOffset < left.startOffset - maximumGap) continue
          if (right.startOffset > left.endOffset + maximumGap) break

          const offset = proposedOffset(left, right)
          if (offset !== null) hypotheses.set(offset.toFixed(6), offset)
        }
      }
    }
  }

  return [...hypotheses.values()].sort((left, right) => left - right)
}

function buildAssignmentGroups(landmarks, supports) {
  return MATCHABLE_LANDMARK_TYPES.map((type) => {
    const supportLookup = new Map()
    const events = new Map()

    for (const support of supports) {
      if (support.type !== type) continue
      const landmarkSupports = supportLookup.get(support.landmarkId) ?? new Map()
      landmarkSupports.set(support.eventId, support)
      supportLookup.set(support.landmarkId, landmarkSupports)
      events.set(support.eventId, support.event)
    }

    return {
      landmarks: landmarks.filter((landmark) => landmark.type === type),
      events: [...events.values()],
      supportLookup,
    }
  })
}

function assignGroupAtOffset({ landmarks, events, supportLookup }, offset) {
  const table = Array.from({ length: landmarks.length + 1 }, () => Array.from({ length: events.length + 1 }, () => null))
  table[0][0] = { totalQuality: 0, totalResidualSeconds: 0, matches: [] }

  for (let landmarkIndex = 0; landmarkIndex <= landmarks.length; landmarkIndex += 1) {
    for (let eventIndex = 0; eventIndex <= events.length; eventIndex += 1) {
      if (landmarkIndex === 0 && eventIndex === 0) continue
      let assignment = null

      if (landmarkIndex > 0) assignment = keepStrongerAssignment(assignment, table[landmarkIndex - 1][eventIndex])
      if (eventIndex > 0) assignment = keepStrongerAssignment(assignment, table[landmarkIndex][eventIndex - 1])
      if (landmarkIndex === 0 || eventIndex === 0) {
        table[landmarkIndex][eventIndex] = assignment
        continue
      }

      const landmark = landmarks[landmarkIndex - 1]
      const event = events[eventIndex - 1]
      const support = supportLookup.get(landmark.id)?.get(event.id)
      if (support !== undefined) {
        const previous = table[landmarkIndex - 1][eventIndex - 1]
        const residual = calculateOffsetResidual(support, offset)
        const quality = calculateTimingQuality(residual, support.timingScaleSeconds)
        assignment = keepStrongerAssignment(assignment, {
          totalQuality: previous.totalQuality + quality,
          totalResidualSeconds: previous.totalResidualSeconds + residual,
          matches: [...previous.matches, { support, residual, quality }],
        })
      }
      table[landmarkIndex][eventIndex] = assignment
    }
  }

  return table[landmarks.length][events.length]
}

function findBestAssignment(offset, groups, eligibleCount) {
  const assignment = { totalQuality: 0, totalResidualSeconds: 0, matches: [] }

  for (const group of groups) {
    const groupAssignment = assignGroupAtOffset(group, offset)
    assignment.totalQuality += groupAssignment.totalQuality
    assignment.totalResidualSeconds += groupAssignment.totalResidualSeconds
    assignment.matches.push(...groupAssignment.matches)
  }

  assignment.score = calculateMatchScore(assignment.matches, eligibleCount)
  return assignment.score.matchedCount >= 2 ? assignment : null
}

function createCandidateRecord(offset, assignment, eligibleCount) {
  const evidence = assignment.matches
    .filter(({ quality }) => quality >= VIDEO_SYNC_MIN_MEANINGFUL_QUALITY)
    .map(({ support, residual, quality }) => ({
      landmarkId: support.landmarkId,
      eventId: support.eventId,
      type: support.type,
      residualSeconds: residual,
      quality,
    }))

  return {
    candidate: {
      variant: 'ordinary',
      offset,
      matchScore: assignment.score.matchScore,
      matchedCount: evidence.length,
      eligibleCount,
      evidence,
      locationClassification: 'none',
    },
    score: assignment.score,
  }
}

function compareCandidateRecords(left, right) {
  return compareCandidateStrength({ ...left.score, offset: left.candidate.offset }, { ...right.score, offset: right.candidate.offset })
}

function mergeCandidateRecords(records) {
  const byOffset = [...records].sort((left, right) => left.candidate.offset - right.candidate.offset)
  const merged = []

  for (const record of byOffset) {
    const previous = merged.at(-1)
    if (previous === undefined || record.candidate.offset - previous.candidate.offset > VIDEO_SYNC_CANDIDATE_MERGE_TOLERANCE_SECONDS) {
      merged.push(record)
    } else if (compareCandidateRecords(record, previous) < 0) {
      merged[merged.length - 1] = record
    }
  }

  return merged.sort(compareCandidateRecords)
}

function classifyCandidateWithLocation(record, locationLandmark, locationOffset) {
  const agrees = Math.abs(record.candidate.offset - locationOffset) <= VIDEO_SYNC_LOCATION_TIMING_TOLERANCE_SECONDS
  return {
    ...record,
    candidate: {
      ...record.candidate,
      variant: agrees ? 'ordinary' : 'locationConflict',
      locationLandmarkId: locationLandmark.id,
      locationOffset,
      locationClassification: agrees ? 'agrees' : 'conflict',
      ...(agrees ? {} : { excludedLandmarkIds: [locationLandmark.id] }),
    },
  }
}

function matchAllLandmarks(landmarks, detection) {
  const { eligibleLandmarks, supports } = createVideoSyncOffsetSupports(landmarks, detection)
  const groups = buildAssignmentGroups(eligibleLandmarks, supports)
  const records = []

  for (const offset of createOffsetHypotheses(eligibleLandmarks, supports)) {
    const assignment = findBestAssignment(offset, groups, eligibleLandmarks.length)
    if (assignment !== null) records.push(createCandidateRecord(offset, assignment, eligibleLandmarks.length))
  }

  const merged = mergeCandidateRecords(records)
  const locationLandmark =
    detection.location === null ? undefined : landmarks.find((landmark) => landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION)
  const locationOffset = locationLandmark === undefined ? null : detection.location.time - locationLandmark.videoSecond
  const classified =
    locationLandmark === undefined ? merged : merged.map((record) => classifyCandidateWithLocation(record, locationLandmark, locationOffset))
  const candidates = classified.slice(0, VIDEO_SYNC_MAX_CANDIDATES).map((record) => record.candidate)

  return {
    candidates,
  }
}

function matchLocation(landmarks, detection) {
  const locationLandmark = landmarks.find((landmark) => landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION)
  if (locationLandmark === undefined) throw new Error('Location-only sync requires a video location landmark')
  if (detection.location === null) throw new Error('Location-only sync requires a detected activity location')

  const offset = detection.location.time - locationLandmark.videoSecond
  const candidate = {
    variant: 'locationOnly',
    offset,
    matchScore: null,
    matchedCount: 1,
    eligibleCount: 1,
    evidence: [
      {
        landmarkId: locationLandmark.id,
        eventId: detection.location.id,
        type: VIDEO_SYNC_LANDMARK_TYPES.LOCATION,
        residualSeconds: 0,
        quality: 1,
      },
    ],
  }

  return {
    candidates: [candidate],
  }
}

/**
 * Finds deterministic video-sync candidates for one explicit matching scope.
 * All-landmark matching includes a resolved location as ordinary evidence;
 * location-only matching returns its single exact alignment.
 *
 * @param {object} input Canonical matching input.
 * @param {object[]} input.landmarks Canonical video landmarks.
 * @param {{availability: {speed: boolean, heading: boolean}, stops: object[], turns: object[], location: object|null}} input.detection Canonical detected activity events.
 * @param {'all'|'location'} input.scope Matching scope.
 * @returns {{candidates: object[]}} Matching candidates.
 */
export function matchVideoSyncCandidates({ landmarks, detection, scope }) {
  if (scope === VIDEO_SYNC_MATCH_SCOPES.ALL) return matchAllLandmarks(landmarks, detection)
  if (scope === VIDEO_SYNC_MATCH_SCOPES.LOCATION) return matchLocation(landmarks, detection)
  throw new Error(`Unsupported video sync match scope: ${String(scope)}`)
}

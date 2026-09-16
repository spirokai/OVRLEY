import {
  VIDEO_SYNC_CANDIDATE_MERGE_TOLERANCE_SECONDS,
  VIDEO_SYNC_LANDMARK_TYPES,
  VIDEO_SYNC_MAX_CANDIDATES,
  VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
} from '../data/videoSyncConstants'
import { calculateMatchScore, compareCandidateStrength } from './matchScore'
import { createVideoSyncOffsetSupports, MATCHABLE_LANDMARK_TYPES, offsetIsSupported, serializeOffsetSupport } from './landmarkTiming'

function findConsensusRegions(supports) {
  const boundaries = new Map()
  for (const support of supports) {
    const start = boundaries.get(support.startOffset) ?? { starts: [], ends: [] }
    start.starts.push(support)
    boundaries.set(support.startOffset, start)

    const end = boundaries.get(support.endOffset) ?? { starts: [], ends: [] }
    end.ends.push(support)
    boundaries.set(support.endOffset, end)
  }

  const offsets = [...boundaries.keys()].sort((left, right) => left - right)
  const activeCounts = new Map()
  const regions = []

  for (let index = 0; index < offsets.length; index += 1) {
    const start = offsets[index]
    const boundary = boundaries.get(start)
    for (const support of boundary.starts) {
      activeCounts.set(support.landmarkId, (activeCounts.get(support.landmarkId) ?? 0) + 1)
    }
    if (activeCounts.size >= 2) regions.push({ start, end: start })

    for (const support of boundary.ends) {
      const nextCount = activeCounts.get(support.landmarkId) - 1
      if (nextCount === 0) activeCounts.delete(support.landmarkId)
      else activeCounts.set(support.landmarkId, nextCount)
    }
    const end = offsets[index + 1]
    if (end !== undefined && activeCounts.size >= 2) regions.push({ start, end })
  }

  return regions
}

function keepStrongerAssignment(assignments, matchCount, candidate) {
  const current = assignments.get(matchCount)
  if (current === undefined) {
    assignments.set(matchCount, candidate)
    return
  }

  const candidateError = candidate.residuals.reduce((total, residual) => total + residual ** 2, 0)
  const currentError = current.residuals.reduce((total, residual) => total + residual ** 2, 0)
  const candidateKey = candidate.matches.map(({ support }) => `${support.landmarkId}:${support.eventId}`).join('|')
  const currentKey = current.matches.map(({ support }) => `${support.landmarkId}:${support.eventId}`).join('|')
  if (candidateError < currentError || (candidateError === currentError && candidateKey.localeCompare(currentKey) < 0)) {
    assignments.set(matchCount, candidate)
  }
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
      events: [...events.values()].sort((left, right) => {
        const leftSecond = left.type === VIDEO_SYNC_LANDMARK_TYPES.STOP ? left.time : left.start
        const rightSecond = right.type === VIDEO_SYNC_LANDMARK_TYPES.STOP ? right.time : right.start
        return leftSecond - rightSecond || left.id.localeCompare(right.id)
      }),
      supportLookup,
    }
  })
}

function assignGroupAtOffset({ landmarks, events, supportLookup }, offset) {
  const table = Array.from({ length: landmarks.length + 1 }, () => Array.from({ length: events.length + 1 }, () => new Map()))
  table[0][0].set(0, { residuals: [], matches: [] })

  for (let landmarkIndex = 0; landmarkIndex <= landmarks.length; landmarkIndex += 1) {
    for (let eventIndex = 0; eventIndex <= events.length; eventIndex += 1) {
      if (landmarkIndex === 0 && eventIndex === 0) continue
      const assignments = table[landmarkIndex][eventIndex]

      if (landmarkIndex > 0) {
        for (const [count, assignment] of table[landmarkIndex - 1][eventIndex]) {
          keepStrongerAssignment(assignments, count, assignment)
        }
      }
      if (eventIndex > 0) {
        for (const [count, assignment] of table[landmarkIndex][eventIndex - 1]) {
          keepStrongerAssignment(assignments, count, assignment)
        }
      }
      if (landmarkIndex === 0 || eventIndex === 0) continue

      const landmark = landmarks[landmarkIndex - 1]
      const event = events[eventIndex - 1]
      const support = supportLookup.get(landmark.id)?.get(event.id)
      if (support === undefined || !offsetIsSupported(support, offset)) continue

      const residual = support.residualAt(offset)
      for (const [count, assignment] of table[landmarkIndex - 1][eventIndex - 1]) {
        keepStrongerAssignment(assignments, count + 1, {
          residuals: [...assignment.residuals, residual],
          matches: [...assignment.matches, { support, residual }],
        })
      }
    }
  }

  return table[landmarks.length][events.length]
}

function findBestAssignment(offset, groups, eligibleCount) {
  let combined = new Map([[0, { residuals: [], matches: [] }]])

  for (const group of groups) {
    const next = new Map()
    for (const [combinedCount, combinedAssignment] of combined) {
      for (const [groupCount, groupAssignment] of assignGroupAtOffset(group, offset)) {
        keepStrongerAssignment(next, combinedCount + groupCount, {
          residuals: [...combinedAssignment.residuals, ...groupAssignment.residuals],
          matches: [...combinedAssignment.matches, ...groupAssignment.matches],
        })
      }
    }
    combined = next
  }

  const qualifying = [...combined.entries()]
    .filter(([matchCount]) => matchCount >= 2 && (eligibleCount !== 2 || matchCount === 2))
    .map(([, assignment]) => ({ ...assignment, score: calculateMatchScore(assignment.residuals, eligibleCount) }))
    .sort((left, right) => compareCandidateStrength({ ...left.score, offset }, { ...right.score, offset }))

  return qualifying[0] ?? null
}

function refinementAnchor(support, offset) {
  if (support.type === VIDEO_SYNC_LANDMARK_TYPES.STOP) return support.preferredOffsets[0]
  const [start, end] = support.preferredOffsets
  if (offset < start) return start
  if (offset > end) return end
  return null
}

function refineRegion(region, groups, eligibleCount) {
  let offset = (region.start + region.end) / 2
  let assignment = findBestAssignment(offset, groups, eligibleCount)
  if (assignment === null || region.start === region.end) return assignment === null ? null : { offset, assignment }

  for (let iteration = 0; iteration <= eligibleCount; iteration += 1) {
    const anchors = assignment.matches.map(({ support }) => refinementAnchor(support, offset)).filter((anchor) => anchor !== null)
    if (anchors.length === 0) break

    const average = anchors.reduce((total, anchor) => total + anchor, 0) / anchors.length
    const refinedOffset = Math.max(region.start, Math.min(region.end, average))
    if (refinedOffset === offset) break

    const refinedAssignment = findBestAssignment(refinedOffset, groups, eligibleCount)
    if (refinedAssignment === null) break
    offset = refinedOffset
    assignment = refinedAssignment
  }

  return { offset, assignment }
}

function createCandidateRecord({ offset, assignment }, eligibleCount) {
  const evidence = assignment.matches.map(({ support, residual }) => ({
    landmarkId: support.landmarkId,
    eventId: support.eventId,
    type: support.type,
    residualSeconds: residual,
  }))

  return {
    candidate: {
      variant: 'ordinary',
      offset,
      matchScore: assignment.score.matchScore,
      matchedCount: evidence.length,
      eligibleCount,
      evidence,
      mapClassification: 'none',
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

function classifyCandidateWithMap(record, mapLandmark, mapOffset) {
  const agrees = Math.abs(record.candidate.offset - mapOffset) <= VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS
  return {
    ...record,
    candidate: {
      ...record.candidate,
      variant: agrees ? 'ordinary' : 'mapConflict',
      mapLandmarkId: mapLandmark.id,
      mapOffset,
      mapClassification: agrees ? 'agrees' : 'conflict',
      ...(agrees ? {} : { excludedLandmarkIds: [mapLandmark.id] }),
    },
  }
}

function createMapOnlyCandidate(mapLandmark, mapOffset, eligibleCount) {
  return {
    variant: 'mapOnly',
    offset: mapOffset,
    matchScore: null,
    matchedCount: 0,
    eligibleCount,
    evidence: [],
    mapLandmarkId: mapLandmark.id,
    mapOffset,
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
  const groups = buildAssignmentGroups(eligibleLandmarks, supports)
  const records = []

  for (const region of findConsensusRegions(supports)) {
    const refined = refineRegion(region, groups, eligibleLandmarks.length)
    if (refined !== null) records.push(createCandidateRecord(refined, eligibleLandmarks.length))
  }

  const merged = mergeCandidateRecords(records)
  const mapLandmark = landmarks.find((landmark) => landmark.type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION && landmark.activitySecond !== null)
  const mapOffset = mapLandmark === undefined ? null : mapLandmark.activitySecond - mapLandmark.videoSecond
  const classified = mapLandmark === undefined ? merged : merged.map((record) => classifyCandidateWithMap(record, mapLandmark, mapOffset))
  const candidateLimit = mapLandmark === undefined ? VIDEO_SYNC_MAX_CANDIDATES : VIDEO_SYNC_MAX_CANDIDATES - 1
  const candidates = classified.slice(0, candidateLimit).map((record) => record.candidate)

  if (mapLandmark !== undefined) {
    candidates.unshift(createMapOnlyCandidate(mapLandmark, mapOffset, eligibleLandmarks.length))
  }

  return {
    candidates,
    diagnostics: {
      eligibleLandmarkIds: eligibleLandmarks.map((landmark) => landmark.id),
      offsetSupports: supports.map(serializeOffsetSupport),
      hypotheses: merged.map((record) => ({
        offset: record.candidate.offset,
        ...record.score,
        evidence: record.candidate.evidence,
      })),
    },
  }
}

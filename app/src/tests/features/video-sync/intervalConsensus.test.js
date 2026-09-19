import { describe, expect, test } from 'vitest'
import { calculateMatchScore } from '@/features/video-sync/utils/matchScore'
import { matchVideoSyncCandidates } from '@/features/video-sync/utils/intervalConsensus'

const ALL_METRICS = { speed: true, heading: true, course: true }

function detection({ location = null, stops = [], turns = [] } = {}) {
  return {
    availability: ALL_METRICS,
    location,
    stops,
    turns,
  }
}

function stop(id, time) {
  return { id, type: 'stop', time, lowSpeedInterval: { start: time, end: time + 2 } }
}

function turn(id, type, start, end) {
  return { id, type, start, end, signedChange: type === 'rightTurn' ? 90 : -90, representativeTime: (start + end) / 2 }
}

describe('manual video-sync interval consensus', () => {
  test('aligns a stop and same-direction turn and rejects the opposite direction', () => {
    const landmarks = [
      { id: 'video-stop', type: 'stop', videoSecond: 10 },
      { id: 'video-right', type: 'rightTurn', videoSecond: 20 },
    ]
    const result = matchVideoSyncCandidates(
      landmarks,
      detection({
        stops: [stop('activity-stop', 110)],
        turns: [turn('activity-left', 'leftTurn', 119, 123), turn('activity-right', 'rightTurn', 119, 123)],
      }),
    )

    expect(result.candidates[0]).toMatchObject({ offset: 100, matchScore: 100, matchedCount: 2, eligibleCount: 2 })
    expect(result.candidates[0].evidence.map((item) => item.eventId)).toEqual(['activity-stop', 'activity-right'])
    expect(result.diagnostics.offsetSupports).not.toContainEqual(expect.objectContaining({ eventId: 'activity-left' }))
  })

  test('prevents one activity event from explaining two landmarks and penalizes partial coverage', () => {
    const twoLandmarkResult = matchVideoSyncCandidates(
      [
        { id: 'video-stop-1', type: 'stop', videoSecond: 0 },
        { id: 'video-stop-2', type: 'stop', videoSecond: 1 },
      ],
      detection({ stops: [stop('activity-stop', 100)] }),
    )
    expect(twoLandmarkResult.candidates).toEqual([])

    const partialResult = matchVideoSyncCandidates(
      [
        { id: 'video-stop-1', type: 'stop', videoSecond: 0 },
        { id: 'video-stop-2', type: 'stop', videoSecond: 1 },
        { id: 'video-stop-3', type: 'stop', videoSecond: 20 },
      ],
      detection({ stops: [stop('activity-stop-1', 100), stop('activity-stop-2', 101)] }),
    )

    expect(partialResult.candidates[0]).toMatchObject({ matchedCount: 2, eligibleCount: 3, matchScore: 67 })
  })

  test('uses absolute score and deterministic ordering, merging, and limit', () => {
    expect(calculateMatchScore([0.25, 0.25], 2)).toMatchObject({
      chiSquare: 0.03125,
      coverage: 1,
      matchScore: 99,
    })

    const landmarks = [
      { id: 'video-stop-1', type: 'stop', videoSecond: 0 },
      { id: 'video-stop-2', type: 'stop', videoSecond: 10 },
    ]
    const events = [100, 110, 200, 210, 300, 310, 400, 410, 500, 510, 600, 610].map((time, index) => stop(`activity-stop-${index}`, time))
    const first = matchVideoSyncCandidates(landmarks, detection({ stops: events }))
    const second = matchVideoSyncCandidates(landmarks, detection({ stops: events }))

    expect(first).toEqual(second)
    expect(first.candidates).toHaveLength(5)
    expect(first.candidates).toEqual([...first.candidates].sort((left, right) => right.matchScore - left.matchScore || left.offset - right.offset))
  })

  test('ignores unresolved locations and uses a resolved location in map-only, combined, and conflict candidates', () => {
    const ordinaryLandmarks = [
      { id: 'video-stop-1', type: 'stop', videoSecond: 10 },
      { id: 'video-stop-2', type: 'stop', videoSecond: 20 },
    ]
    const unresolved = matchVideoSyncCandidates(
      [...ordinaryLandmarks, { id: 'video-location', type: 'location', videoSecond: 2, activitySecond: null }],
      detection(),
    )
    expect(unresolved.candidates).toEqual([])
    expect(unresolved.diagnostics.eligibleLandmarkIds).toEqual(['video-stop-1', 'video-stop-2'])

    const resolved = matchVideoSyncCandidates(
      [...ordinaryLandmarks, { id: 'video-location', type: 'location', videoSecond: 0, activitySecond: null }],
      detection({ location: { id: 'map-location', type: 'location', time: 0 }, stops: [stop('activity-stop-1', 20), stop('activity-stop-2', 30)] }),
    )
    expect(resolved.candidates[0]).toMatchObject({ variant: 'mapOnly', offset: 0, matchScore: null, mapClassification: 'authoritative' })
    expect(resolved.candidates[1]).toMatchObject({ variant: 'ordinary', offset: 0, matchedCount: 2, eligibleCount: 3, mapClassification: 'agrees' })
    expect(resolved.candidates[1].evidence.map((item) => item.type)).toEqual(['stop', 'location'])
    expect(resolved.diagnostics.eligibleLandmarkIds).toContain('video-location')

    const conflicting = matchVideoSyncCandidates(
      [...ordinaryLandmarks, { id: 'video-location', type: 'location', videoSecond: 0, activitySecond: null }],
      detection({ location: { id: 'map-location', type: 'location', time: 0 }, stops: [stop('activity-stop-1', 30), stop('activity-stop-2', 40)] }),
    )
    expect(conflicting.candidates.find((candidate) => candidate.variant === 'mapConflict')).toMatchObject({
      variant: 'mapConflict',
      offset: 20,
      mapClassification: 'conflict',
      excludedLandmarkIds: ['video-location'],
    })
  })
})

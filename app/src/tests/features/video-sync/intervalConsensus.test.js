import { describe, expect, test } from 'vitest'
import { VIDEO_SYNC_MATCH_SCOPES } from '@/features/video-sync/data/videoSyncConstants'
import { calculateMatchScore, calculateTimingQuality } from '@/features/video-sync/utils/matchScore'
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
    const result = matchVideoSyncCandidates({
      landmarks,
      detection: detection({
        stops: [stop('activity-stop', 110)],
        turns: [turn('activity-left', 'leftTurn', 119, 123), turn('activity-right', 'rightTurn', 119, 123)],
      }),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })

    expect(result.candidates[0]).toMatchObject({ offset: 101, matchScore: 100, matchedCount: 2, eligibleCount: 2 })
    expect(result.candidates[0].evidence.map((item) => item.eventId)).toEqual(['activity-stop', 'activity-right'])
  })

  test('prevents one activity event from explaining two landmarks and penalizes partial coverage', () => {
    const twoLandmarkResult = matchVideoSyncCandidates({
      landmarks: [
        { id: 'video-stop-1', type: 'stop', videoSecond: 0 },
        { id: 'video-stop-2', type: 'stop', videoSecond: 1 },
      ],
      detection: detection({ stops: [stop('activity-stop', 100)] }),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })
    expect(twoLandmarkResult.candidates).toEqual([])

    const partialResult = matchVideoSyncCandidates({
      landmarks: [
        { id: 'video-stop-1', type: 'stop', videoSecond: 0 },
        { id: 'video-stop-2', type: 'stop', videoSecond: 1 },
        { id: 'video-stop-3', type: 'stop', videoSecond: 20 },
      ],
      detection: detection({ stops: [stop('activity-stop-1', 100), stop('activity-stop-2', 101)] }),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })

    expect(partialResult.candidates[0]).toMatchObject({ matchedCount: 2, eligibleCount: 3, matchScore: 67 })
  })

  test('scores interval misses continuously and preserves deterministic ordering, merging, and limit', () => {
    expect(calculateTimingQuality(0, 2)).toBe(1)
    expect(calculateTimingQuality(2, 2)).toBe(0.5)
    expect(calculateTimingQuality(4, 2)).toBe(0.2)
    expect(
      calculateMatchScore(
        [
          { residual: 0, quality: 1 },
          { residual: 2, quality: 0.5 },
        ],
        2,
      ),
    ).toMatchObject({
      totalQuality: 1.5,
      coverage: 1,
      matchScore: 75,
    })

    const landmarks = [
      { id: 'video-stop-1', type: 'stop', videoSecond: 0 },
      { id: 'video-stop-2', type: 'stop', videoSecond: 10 },
    ]
    const events = [100, 110, 200, 210, 300, 310, 400, 410, 500, 510, 600, 610].map((time, index) => stop(`activity-stop-${index}`, time))
    const first = matchVideoSyncCandidates({ landmarks, detection: detection({ stops: events }), scope: VIDEO_SYNC_MATCH_SCOPES.ALL })
    const second = matchVideoSyncCandidates({ landmarks, detection: detection({ stops: events }), scope: VIDEO_SYNC_MATCH_SCOPES.ALL })

    expect(first).toEqual(second)
    expect(first.candidates).toHaveLength(5)
    expect(first.candidates).toEqual([...first.candidates].sort((left, right) => right.matchScore - left.matchScore || left.offset - right.offset))
  })

  test('treats detected periods as flat and retains nearby non-overlapping evidence', () => {
    const flat = matchVideoSyncCandidates({
      landmarks: [
        { id: 'video-stop-1', type: 'stop', videoSecond: 0 },
        { id: 'video-stop-2', type: 'stop', videoSecond: 10 },
      ],
      detection: detection({ stops: [stop('activity-stop-1', 100), stop('activity-stop-2', 110)] }),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })
    expect(flat.candidates[0]).toMatchObject({ offset: 101, matchScore: 100, matchedCount: 2 })

    const nearMiss = matchVideoSyncCandidates({
      landmarks: [
        { id: 'video-stop', type: 'stop', videoSecond: 0 },
        { id: 'video-turn', type: 'rightTurn', videoSecond: 10 },
      ],
      detection: detection({
        stops: [stop('activity-stop', 100)],
        turns: [turn('activity-turn', 'rightTurn', 116, 118)],
      }),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })
    expect(nearMiss.candidates[0]).toMatchObject({ offset: 104, matchScore: 50, matchedCount: 2 })
    expect(nearMiss.candidates[0].evidence.map(({ residualSeconds }) => residualSeconds)).toEqual([2, 2])
  })

  test('keeps location-only matching independent from combined and conflict candidates', () => {
    const ordinaryLandmarks = [
      { id: 'video-stop-1', type: 'stop', videoSecond: 10 },
      { id: 'video-stop-2', type: 'stop', videoSecond: 20 },
    ]
    const unresolved = matchVideoSyncCandidates({
      landmarks: [...ordinaryLandmarks, { id: 'video-location', type: 'location', videoSecond: 2, activitySecond: null }],
      detection: detection(),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })
    expect(unresolved.candidates).toEqual([])

    const resolved = matchVideoSyncCandidates({
      landmarks: [...ordinaryLandmarks, { id: 'video-location', type: 'location', videoSecond: 0, activitySecond: null }],
      detection: detection({
        location: { id: 'map-location', type: 'location', time: 0 },
        stops: [stop('activity-stop-1', 20), stop('activity-stop-2', 30)],
      }),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })
    expect(
      matchVideoSyncCandidates({
        landmarks: [...ordinaryLandmarks, { id: 'video-location', type: 'location', videoSecond: 0 }],
        detection: detection({ location: { id: 'map-location', type: 'location', time: 0 } }),
        scope: VIDEO_SYNC_MATCH_SCOPES.LOCATION,
      }).candidates[0],
    ).toMatchObject({
      variant: 'locationOnly',
      offset: 0,
    })
    expect(resolved.candidates.every((candidate) => candidate.variant !== 'locationOnly')).toBe(true)
    expect(resolved.candidates.find((candidate) => candidate.locationClassification === 'agrees')).toMatchObject({
      variant: 'ordinary',
      locationClassification: 'agrees',
    })

    const conflicting = matchVideoSyncCandidates({
      landmarks: [...ordinaryLandmarks, { id: 'video-location', type: 'location', videoSecond: 0, activitySecond: null }],
      detection: detection({
        location: { id: 'map-location', type: 'location', time: 0 },
        stops: [stop('activity-stop-1', 30), stop('activity-stop-2', 40)],
      }),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })
    expect(conflicting.candidates.find((candidate) => candidate.variant === 'locationConflict')).toMatchObject({
      variant: 'locationConflict',
      offset: 21,
      locationClassification: 'conflict',
      excludedLandmarkIds: ['video-location'],
    })
  })

  test('uses location as evidence when syncing all landmarks', () => {
    const result = matchVideoSyncCandidates({
      landmarks: [
        { id: 'video-location', type: 'location', videoSecond: 0, activitySecond: null },
        { id: 'video-stop', type: 'stop', videoSecond: 10 },
      ],
      detection: detection({
        location: { id: 'map-location', type: 'location', time: 100 },
        stops: [stop('activity-stop', 110)],
      }),
      scope: VIDEO_SYNC_MATCH_SCOPES.ALL,
    })

    expect(result.candidates[0]).toMatchObject({ offset: 100, matchedCount: 2, eligibleCount: 2, locationClassification: 'agrees' })
    expect(result.candidates[0].evidence.map((item) => item.type)).toEqual(['stop', 'location'])
  })

  test('rejects an unknown matching scope', () => {
    expect(() => matchVideoSyncCandidates({ landmarks: [], detection: detection(), scope: 'mapOnly' })).toThrow(/Unsupported video sync match scope/)
  })
})

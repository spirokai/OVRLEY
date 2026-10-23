import { describe, expect, test } from 'vitest'
import { findDefaultIncludedIds, findFarVideos, findLargestStartGroup } from '@/features/video-preview/utils/importReview'

function clip(id, offsetSeconds, durationSeconds = 60) {
  return { id, offsetSeconds, durationSeconds }
}

function far(options) {
  const { kind, farIds } = findFarVideos({ undeterminedIds: new Set(), activityDurationSeconds: null, ...options })
  return { kind, farIds: [...farIds].sort() }
}

function defaults(options) {
  return [...findDefaultIncludedIds({ undeterminedIds: new Set(), activityDurationSeconds: null, ...options })].sort()
}

describe('findFarVideos', () => {
  test('with an activity, only the start of a video counts, up to an hour before the start or after the end', () => {
    const clips = [
      clip('edge-before', -3600),
      clip('too-early', -3601),
      clip('edge-after', 7200 + 3600),
      clip('too-late', 7201 + 3600),
      clip('long-before', -3700, 600),
      clip('unknown', 99999),
    ]
    const ids = new Set(clips.map((candidate) => candidate.id))
    // Ends inside the window but starts before it; a video without a recording time is placed by the user.
    expect(far({ clips, newIds: ids, includedIds: ids, undeterminedIds: new Set(['unknown']), activityDurationSeconds: 7200 })).toEqual({
      kind: 'activity',
      farIds: ['long-before', 'too-early', 'too-late'],
    })
  })

  test('without an activity, new videos start within an hour of the switched-on videos already imported', () => {
    const clips = [clip('a', 0, 100), clip('b', 500, 100), clip('before', -3600), clip('after', 600 + 3600), clip('far', 600 + 3601)]
    const newIds = new Set(['before', 'after', 'far'])
    expect(far({ clips, newIds, includedIds: new Set(['a', 'b']) })).toEqual({ kind: 'videos', farIds: ['far'] })
    // Without b the window ends an hour after a.
    expect(far({ clips, newIds, includedIds: new Set(['a']) }).farIds).toEqual(['after', 'far'])
  })

  test('without an activity or videos already imported, videos start within 12 hours of the largest switched-on group', () => {
    const clips = [clip('a', 0), clip('b', 3600), clip('c', 12 * 3600 + 1), clip('d', 40 * 3600)]
    const ids = new Set(clips.map((candidate) => candidate.id))
    expect(far({ clips, newIds: ids, includedIds: ids })).toEqual({ kind: 'span', farIds: ['c', 'd'] })
    expect(far({ clips, newIds: ids, includedIds: new Set() })).toEqual({ kind: 'span', farIds: [] })
  })
})

describe('findDefaultIncludedIds', () => {
  test('switches on the videos already imported and the new videos near the activity whose time is known', () => {
    const clips = [clip('old', 9000), clip('ride', 0), clip('early', -4000), clip('unknown', 60)]
    expect(
      defaults({ clips, newIds: new Set(['ride', 'early', 'unknown']), undeterminedIds: new Set(['unknown']), activityDurationSeconds: 120 }),
    ).toEqual(['old', 'ride'])
  })

  test('without a reference, switches on the largest group of new videos starting within 12 hours', () => {
    const clips = [clip('lone', 0), clip('a', 20 * 3600), clip('b', 21 * 3600), clip('c', 30 * 3600)]
    expect(defaults({ clips, newIds: new Set(clips.map((candidate) => candidate.id)) })).toEqual(['a', 'b', 'c'])
  })
})

describe('findLargestStartGroup', () => {
  test('finds the starts of the largest group, the earliest on a tie', () => {
    expect(findLargestStartGroup([clip('a', 50), clip('b', 0), clip('c', 200)], 100)).toEqual({ from: 0, to: 50 })
    expect(findLargestStartGroup([clip('a', 0), clip('b', 500)], 100)).toEqual({ from: 0, to: 0 })
    expect(findLargestStartGroup([], 100)).toBeNull()
  })
})

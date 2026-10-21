import { describe, expect, test } from 'vitest'
import { reviewImportWindow } from '@/features/video-preview/utils/importReview'

function clip(id, offsetSeconds, durationSeconds = 60) {
  return { id, offsetSeconds, durationSeconds }
}

function verdicts(options) {
  return Object.fromEntries(reviewImportWindow({ undeterminedIds: new Set(), activityDurationSeconds: null, ...options }).verdicts)
}

describe('reviewImportWindow', () => {
  test('with an activity, only the start of a video counts, up to an hour before the start or after the end', () => {
    const clips = [
      clip('edge-before', -3600),
      clip('too-early', -3601),
      clip('edge-after', 7200 + 3600),
      clip('too-late', 7201 + 3600),
      clip('long-before', -3700, 600),
    ]
    expect(verdicts({ clips, newIds: new Set(clips.map((candidate) => candidate.id)), activityDurationSeconds: 7200 })).toEqual({
      'edge-before': 'import',
      'too-early': 'excluded',
      'edge-after': 'import',
      'too-late': 'excluded',
      // Ends inside the window but starts before it.
      'long-before': 'excluded',
    })
  })

  test('with an activity, a video already imported stays and is flagged outside the window', () => {
    const result = reviewImportWindow({
      clips: [clip('old', 9000), clip('new', 0)],
      newIds: new Set(['new']),
      undeterminedIds: new Set(),
      activityDurationSeconds: 120,
    })
    expect(result.kind).toBe('activity')
    expect(Object.fromEntries(result.verdicts)).toEqual({ old: 'outside', new: 'import' })
  })

  test('without an activity, new videos start within an hour of the first start or the last end of the existing videos', () => {
    const clips = [clip('a', 0, 100), clip('b', 500, 100), clip('before', -3600), clip('after', 600 + 3600), clip('far', 600 + 3601)]
    expect(verdicts({ clips, newIds: new Set(['before', 'after', 'far']) })).toEqual({
      a: 'kept',
      b: 'kept',
      before: 'import',
      after: 'import',
      far: 'excluded',
    })
  })

  test('a fresh import without an activity takes all videos within 12 hours of each other, or none', () => {
    const newIds = new Set(['a', 'b', 'unknown'])
    expect(verdicts({ clips: [clip('a', 0), clip('b', 12 * 3600), clip('unknown', 0)], newIds, undeterminedIds: new Set(['unknown']) })).toEqual({
      a: 'import',
      b: 'import',
      unknown: 'import',
    })
    expect(verdicts({ clips: [clip('a', 0), clip('b', 12 * 3600 + 1), clip('unknown', 0)], newIds, undeterminedIds: new Set(['unknown']) })).toEqual({
      a: 'excluded',
      b: 'excluded',
      unknown: 'excluded',
    })
  })

  test('a new video without a recording time is placed by the user and joins', () => {
    expect(
      verdicts({
        clips: [clip('ride', 0), clip('unknown', 99999)],
        newIds: new Set(['unknown']),
        undeterminedIds: new Set(['unknown']),
        activityDurationSeconds: 120,
      }),
    ).toEqual({ ride: 'kept', unknown: 'import' })
  })
})

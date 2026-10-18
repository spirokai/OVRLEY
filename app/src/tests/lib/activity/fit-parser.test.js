import FitParser from 'fit-file-parser'
import { afterEach, describe, expect, test, vi } from 'vitest'

import parseFitActivityFile from '@/lib/activity/fit-parser'

const origin = Date.parse('2026-01-01T00:00:00Z')
const timestampAt = (seconds) => new Date(origin + seconds * 1000)

describe('parseFitActivityFile lap timing', () => {
  afterEach(() => vi.restoreAllMocks())

  test('preserves exact lap boundaries when the first lap starts before the first record', async () => {
    vi.spyOn(FitParser.prototype, 'parseAsync').mockResolvedValue({
      laps: [{ start_time: timestampAt(1) }, { start_time: timestampAt(5.5) }],
      records: [2, 3, 7].map((seconds) => ({ timestamp: timestampAt(seconds), elapsed_time: seconds - 2 })),
    })

    const activity = await parseFitActivityFile({ name: 'laps.fit', arrayBuffer: async () => new ArrayBuffer(0) })

    expect(activity.sync_time).toBe('2026-01-01T00:00:01.000Z')
    expect(activity.lap_markers).toEqual({ BeaconMarkers: [0, 4.5] })
    expect(activity.raw_samples.map((sample) => sample.elapsed_seconds)).toEqual([1, 2, 6])
  })
})

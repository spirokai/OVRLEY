import { describe, expect, test } from 'vite-plus/test'

import { resolveExportRangeWindow } from '@/lib/export-range'

describe('export range activity duration', () => {
  test.each([
    [{ trim_end_seconds: 42, metadata: { duration_seconds: 18 } }, 42],
    [{ metadata: { duration_seconds: 18 } }, 18],
    [null, 0],
  ])('scopes the full range to the available activity duration', (activity, duration) => {
    expect(resolveExportRangeWindow(activity, { type: 'all', from: 0, to: 0 })).toEqual({ active: false, start: 0, end: duration })
  })
})

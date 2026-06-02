/**
 * Behavior tests for `standard-metrics.js` TEXT_DEFAULTS.
 *
 * The `display_type` field on every newly created value widget must default to
 * `"text"` so existing rendering behavior is preserved out of the box. Future
 * slices introduce gauge display types (linear, bars, arc, corner, tape) but
 * the default remains `"text"` for backward compatibility.
 */

import { describe, expect, test } from 'vitest'
import { TEXT_DEFAULTS } from '@/lib/standard-metrics'

describe('TEXT_DEFAULTS', () => {
  test('includes display_type: "text" so new value widgets default to text rendering', () => {
    expect(TEXT_DEFAULTS.display_type).toBe('text')
  })
})

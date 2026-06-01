/**
 * Behavior tests for `widgetDefaults.js`.
 *
 * The `display_type` field on every newly created value widget must default to
 * `"text"` so existing rendering behavior is preserved out of the box. Future
 * slices introduce gauge display types (linear, bars, arc, corner, tape) but
 * the default remains `"text"` for backward compatibility.
 */

import { describe, expect, test } from 'vitest'
import { SHARED_VALUE_DEFAULTS } from '@/features/widget-editor/data/widgetDefaults'

describe('SHARED_VALUE_DEFAULTS', () => {
  test('includes display_type: "text" so new value widgets default to text rendering', () => {
    expect(SHARED_VALUE_DEFAULTS.display_type).toBe('text')
  })
})

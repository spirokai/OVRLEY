/**
 * Characterization test for the useOverlayEditorState hook.
 *
 * Pins down the current public contract so that decomposition
 * cannot accidentally change what callers receive. Verifies the
 * hook is importable as a default export with the expected
 * function signature.
 */

import { describe, expect, test } from 'vitest'
import useOverlayEditorState from '@/features/overlay-editor/hooks/useOverlayEditorState'
import { getPrimarySelectionId, normalizeSelectionIds } from '@/features/overlay-editor/utils/overlayEditorHelpers'

describe('useOverlayEditorState module contract', () => {
  test('exports a function as default', () => {
    expect(typeof useOverlayEditorState).toBe('function')
  })

  test('accepts config, globalDefaults, onConfigChange, zoomLevel, onZoomLevelChange', () => {
    expect(useOverlayEditorState.length).toBe(1)
  })
})

describe('useOverlayEditorState dependencies — selection helpers', () => {
  test('normalizeSelectionIds returns only valid IDs from the ordered list', () => {
    const result = normalizeSelectionIds(['w1', 'w3', 'ghost'], ['w1', 'w2', 'w3'])
    expect(result).toEqual(['w1', 'w3'])
  })

  test('normalizeSelectionIds handles empty input', () => {
    expect(normalizeSelectionIds([], ['w1', 'w2'])).toEqual([])
  })

  test('getPrimarySelectionId picks the last ID when preferred is null', () => {
    expect(getPrimarySelectionId(['w1', 'w2'], null)).toBe('w2')
  })

  test('getPrimarySelectionId uses preferred ID when it exists in the list', () => {
    expect(getPrimarySelectionId(['w1', 'w2'], 'w1')).toBe('w1')
  })

  test('getPrimarySelectionId falls back to last when preferred is not in list', () => {
    expect(getPrimarySelectionId(['w1', 'w2'], 'ghost')).toBe('w2')
  })

  test('getPrimarySelectionId returns null for empty list', () => {
    expect(getPrimarySelectionId([], 'w1')).toBeNull()
  })
})

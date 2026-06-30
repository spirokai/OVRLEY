/**
 * Characterization test for the useOverlayEditorState hook.
 *
 * Pins down the current public contract so that decomposition
 * cannot accidentally change what callers receive. Verifies the
 * hook is importable as a default export with the expected
 * function signature.
 */

import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useOverlayEditorState from '@/features/overlay-editor/hooks/useOverlayEditorState'
import { getPrimarySelectionId, normalizeSelectionIds } from '@/features/overlay-editor/utils/overlayEditorHelpers'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'

function resetStore() {
  useStore.setState(useStore.getInitialState(), true)
}

describe('useOverlayEditorState module contract', () => {
  beforeEach(resetStore)

  test('exports a function as default', () => {
    expect(typeof useOverlayEditorState).toBe('function')
  })

  test('accepts config, globalDefaults, onConfigChange, zoomLevel, onZoomLevelChange', () => {
    expect(useOverlayEditorState.length).toBe(1)
  })

  test('reacts when parsedActivity is replaced after async telemetry load', () => {
    useStore.setState({
      dummyDurationSeconds: 10,
      parsedActivity: null,
      selectedSecond: 9,
    })

    const { result } = renderHook(() =>
      useOverlayEditorState({
        config: DEFAULT_CONFIG,
        globalDefaults: {},
        onConfigChange: vi.fn(),
        zoomLevel: 1,
        onZoomLevelChange: vi.fn(),
      }),
    )

    expect(result.current.previewSecond).toBe(9)

    act(() => {
      useStore.setState({
        parsedActivity: {
          trim_end_seconds: 3,
          metadata: { duration_seconds: 3 },
        },
      })
    })

    expect(result.current.previewSecond).toBe(3)
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

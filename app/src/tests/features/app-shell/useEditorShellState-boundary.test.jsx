import { renderHook } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import useEditorShellState from '@/features/app-shell/hooks/useEditorShellState'

describe('useEditorShellState boundary', () => {
  test('returns default values without throwing in jsdom', () => {
    const { result } = renderHook(() => useEditorShellState())

    expect(result.current.editorZoomLevel).toBe(1)
    expect(result.current.editorBackgroundMode).toBe('checker')
    expect(result.current.editorGridVisible).toBe(false)
    expect(result.current.uiScale).toBeGreaterThan(0)
    expect(typeof result.current.increaseZoom).toBe('function')
    expect(typeof result.current.decreaseZoom).toBe('function')
  })

  test('returns debugModeEnabled without throwing', () => {
    const { result } = renderHook(() => useEditorShellState())
    expect(typeof result.current.debugModeEnabled).toBe('boolean')
  })
})

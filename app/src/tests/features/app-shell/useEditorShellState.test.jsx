/**
 * Regression tests for editor-shell presentation state.
 *
 * The hook used to hydrate and persist editor chrome preferences through
 * `localStorage`. These tests document the new contract: the shell always
 * starts from in-memory defaults and user interactions stay session-local.
 */

import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import useEditorShellState from '@/features/app-shell/hooks/useEditorShellState'

describe('useEditorShellState', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  afterEach(() => {
    vi.restoreAllMocks()
    localStorage.clear()
  })

  test('keeps editor shell preferences in memory instead of restoring or persisting browser storage', () => {
    // Pollute the old persistence keys first so the test proves they are ignored.
    localStorage.setItem('overlayBackgroundMode', 'solid')
    localStorage.setItem('overlayGridVisible', 'true')
    localStorage.setItem('overlaySnapToGrid', 'true')

    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem')
    const { result } = renderHook(() => useEditorShellState())

    // Mount should use the documented session defaults, not browser storage.
    expect(result.current.editorBackgroundMode).toBe('checker')
    expect(result.current.editorGridVisible).toBe(false)
    expect(result.current.editorSnapToGrid).toBe(false)

    act(() => {
      result.current.setEditorBackgroundMode('solid')
      result.current.setEditorGridVisible(true)
      result.current.setEditorSnapToGrid(true)
    })

    // State updates still work, but they remain entirely in React memory.
    expect(result.current.editorBackgroundMode).toBe('solid')
    expect(result.current.editorGridVisible).toBe(true)
    expect(result.current.editorSnapToGrid).toBe(true)
    expect(setItemSpy).not.toHaveBeenCalled()
  })
})

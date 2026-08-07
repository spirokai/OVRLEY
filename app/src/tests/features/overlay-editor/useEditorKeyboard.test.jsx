import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { useEditorKeyboard } from '@/features/overlay-editor/hooks/useEditorKeyboard'

function renderEditorKeyboard(overrides = {}) {
  const onConfigChange = vi.fn()
  const setWidgetSelection = vi.fn()
  const clipboardRef = { current: null }
  const config = {
    backdrops: [{ id: 'widget-1', x: 10, y: 20 }],
    ...overrides.config,
  }
  const selectedWidgets = [
    {
      category: 'backdrops',
      data: config.backdrops[0],
      id: 'widget-1',
      type: 'backdrop',
    },
  ]
  const editorShell = {
    activeKeyboardWorkspace: 'editor',
    decreaseZoom: vi.fn(),
    editorGridVisible: false,
    editorSnapToGrid: false,
    increaseZoom: vi.fn(),
    resetZoom: vi.fn(),
    setEditorGridVisible: vi.fn(),
    setEditorSnapToGrid: vi.fn(),
  }

  const hook = renderHook(() =>
    useEditorKeyboard({
      clipboardRef,
      config,
      editorShell,
      onConfigChange,
      selectedWidgetIds: ['widget-1'],
      selectedWidgets,
      setWidgetSelection,
    }),
  )

  return { ...hook, clipboardRef, config, editorShell, onConfigChange, setWidgetSelection }
}

describe('useEditorKeyboard', () => {
  test('nudges selected widgets without falling through to playback', () => {
    const { onConfigChange } = renderEditorKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'ArrowRight' }))
    })

    expect(onConfigChange).toHaveBeenCalledWith({
      backdrops: [{ id: 'widget-1', x: 11, y: 20 }],
    })
  })

  test('keeps Mod+V as widget paste', () => {
    const { clipboardRef, config, onConfigChange } = renderEditorKeyboard()
    clipboardRef.current = {
      widgets: [{ category: 'backdrops', data: config.backdrops[0], id: 'widget-1', type: 'backdrop' }],
    }

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, ctrlKey: true, key: 'v' }))
    })

    expect(onConfigChange).toHaveBeenCalledTimes(1)
    expect(onConfigChange.mock.calls[0][0].backdrops).toHaveLength(2)
  })

  test('does not capture shortcuts from inputs', () => {
    const { onConfigChange } = renderEditorKeyboard()
    const input = document.createElement('input')
    document.body.appendChild(input)

    act(() => {
      input.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'ArrowRight' }))
    })

    expect(onConfigChange).not.toHaveBeenCalled()
    input.remove()
  })

  test('routes grid and snapping toggles through shell controls', () => {
    const { editorShell } = renderEditorKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'g' }))
    })

    expect(editorShell.setEditorSnapToGrid).toHaveBeenCalledWith(true)
    expect(editorShell.setEditorGridVisible).toHaveBeenCalledWith(true)
  })
})

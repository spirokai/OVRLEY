import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vite-plus/test'
import { useEditorKeyboard } from '@/features/overlay-editor/hooks/useEditorKeyboard'
import { DEFAULT_CONFIG } from '@/store/store-utils'

function renderEditorKeyboard(overrides = {}) {
  const onConfigChange = vi.fn()
  const setWidgetSelection = vi.fn()
  const clipboardRef = { current: null }
  const config = {
    ...DEFAULT_CONFIG,
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
      ...DEFAULT_CONFIG,
      backdrops: [{ id: 'widget-1', x: 11, y: 20 }],
    })
  })

  test('leaves Mod+C to the browser while text is selected, so an error message can be copied', () => {
    const { clipboardRef } = renderEditorKeyboard()
    const press = () => {
      const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ctrlKey: true, key: 'c' })
      act(() => {
        window.dispatchEvent(event)
      })
      return event
    }
    const selection = vi.spyOn(window, 'getSelection').mockReturnValue({ toString: () => 'Encoding error' })
    const before = clipboardRef.current
    expect(press().defaultPrevented).toBe(false)
    expect(clipboardRef.current).toBe(before)

    selection.mockReturnValue({ toString: () => '' })
    expect(press().defaultPrevented).toBe(true)
    expect(clipboardRef.current.widgets).toHaveLength(1)
    selection.mockRestore()
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

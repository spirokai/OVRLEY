import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { useToolbarDrawer } from '@/features/toolbar'

vi.mock('@/features/toolbar/hooks/useDrawerPreference', () => ({
  useDrawerPreference: () => true,
}))

function createLayout(overrides = {}) {
  return {
    activeLeftDrawerTool: 'widgets',
    dismissLeftDrawerOverlay: vi.fn(),
    leftDrawerPinned: false,
    leftDrawerVisible: true,
    selectLeftDrawerTool: vi.fn(),
    setLeftDrawerPinned: vi.fn(),
    ...overrides,
  }
}

function dispatchEscape() {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: 'Escape' })
  act(() => window.dispatchEvent(event))
  return event
}

describe('useToolbarDrawer', () => {
  test('Escape dismisses a visible unpinned drawer', () => {
    const layout = createLayout()
    renderHook(() => useToolbarDrawer(layout))

    const event = dispatchEscape()

    expect(event.defaultPrevented).toBe(true)
    expect(layout.dismissLeftDrawerOverlay).toHaveBeenCalledOnce()
  })

  test('Escape preserves transient UI in a pinned drawer', () => {
    const layout = createLayout({ leftDrawerPinned: true })
    const transient = document.createElement('div')
    transient.dataset.slot = 'popover-content'
    document.body.appendChild(transient)
    renderHook(() => useToolbarDrawer(layout))

    const event = dispatchEscape()

    expect(event.defaultPrevented).toBe(true)
    expect(layout.dismissLeftDrawerOverlay).not.toHaveBeenCalled()
    transient.remove()
  })

  test('a pinned drawer does not consume Escape when no nested transient UI is open', () => {
    const layout = createLayout({ leftDrawerPinned: true })
    renderHook(() => useToolbarDrawer(layout))

    const event = dispatchEscape()

    expect(event.defaultPrevented).toBe(false)
    expect(layout.dismissLeftDrawerOverlay).not.toHaveBeenCalled()
  })
})

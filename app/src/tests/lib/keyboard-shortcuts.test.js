import { afterEach, describe, expect, test } from 'vitest'
import { hasOpenOverlay, hasOpenPopup, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'

afterEach(() => {
  document.body.replaceChildren()
})

function keyEvent(key, modifiers = {}) {
  return {
    altKey: false,
    ctrlKey: false,
    key,
    metaKey: false,
    repeat: false,
    shiftKey: false,
    ...modifiers,
  }
}

describe('keyboard shortcut matcher', () => {
  test('matches Ctrl and Command as mod bindings', () => {
    expect(matchKeyboardShortcut(keyEvent('n', { ctrlKey: true }), 'app')?.commandId).toBe('template.new')
    expect(matchKeyboardShortcut(keyEvent('n', { metaKey: true }), 'app')?.commandId).toBe('template.new')
  })

  test('keeps modifier matching exact for AltGr-like input', () => {
    expect(matchKeyboardShortcut(keyEvent('k', { ctrlKey: true }), 'app')?.commandId).toBe('activity.import')
    expect(matchKeyboardShortcut(keyEvent('k', { altKey: true, ctrlKey: true }), 'app')).toBeNull()
  })

  test('preserves Ctrl+Y redo', () => {
    expect(matchKeyboardShortcut(keyEvent('y', { ctrlKey: true }), 'history')?.commandId).toBe('history.redo')
  })

  test('matches both browser key values for Ctrl++', () => {
    expect(matchKeyboardShortcut(keyEvent('+', { ctrlKey: true, shiftKey: true }), 'editor')?.commandId).toBe('editor.zoomIn')
    expect(matchKeyboardShortcut(keyEvent('=', { ctrlKey: true }), 'editor')?.commandId).toBe('editor.zoomIn')
  })
})

describe('keyboard overlay ownership', () => {
  test.each(['dialog-content', 'select-content', 'popover-content', 'left-drawer-backdrop'])('treats %s as a keyboard-owning overlay', (slot) => {
    const overlay = document.createElement('div')
    overlay.dataset.slot = slot
    document.body.appendChild(overlay)

    expect(hasOpenOverlay()).toBe(true)
  })

  test('only select and popover content count as nested transient UI', () => {
    const drawerBackdrop = document.createElement('div')
    drawerBackdrop.dataset.slot = 'left-drawer-backdrop'
    document.body.appendChild(drawerBackdrop)

    expect(hasOpenPopup()).toBe(false)

    const popover = document.createElement('div')
    popover.dataset.slot = 'popover-content'
    document.body.appendChild(popover)

    expect(hasOpenPopup()).toBe(true)
  })
})

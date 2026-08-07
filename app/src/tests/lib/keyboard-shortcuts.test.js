import { describe, expect, test } from 'vitest'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'

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
    expect(matchKeyboardShortcut(keyEvent('a', { altKey: true }), 'app')?.commandId).toBe('activity.import')
    expect(matchKeyboardShortcut(keyEvent('a', { altKey: true, ctrlKey: true }), 'app')).toBeNull()
  })

  test('preserves Ctrl+Y redo', () => {
    expect(matchKeyboardShortcut(keyEvent('y', { ctrlKey: true }), 'history')?.commandId).toBe('history.redo')
  })

  test('matches both browser key values for Ctrl++', () => {
    expect(matchKeyboardShortcut(keyEvent('+', { ctrlKey: true, shiftKey: true }), 'editor')?.commandId).toBe('editor.zoomIn')
    expect(matchKeyboardShortcut(keyEvent('=', { ctrlKey: true }), 'editor')?.commandId).toBe('editor.zoomIn')
  })
})

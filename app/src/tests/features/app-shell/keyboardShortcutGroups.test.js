import { describe, expect, test } from 'vitest'
import { getKeyboardShortcutGroups } from '@/features/app-shell/utils/keyboardShortcutGroups'

describe('keyboard shortcut help groups', () => {
  test('combines bindings with the same description into one row', () => {
    const groups = getKeyboardShortcutGroups()
    const editorGroup = groups.find((group) => group.name === 'Widget editor')
    const deleteShortcut = editorGroup.shortcuts.find((shortcut) => shortcut.description === 'Delete selected widgets')

    expect(deleteShortcut.options).toEqual([
      {
        groupKey: '',
        keys: ['Del', 'Backspace'],
        modifiers: [],
      },
    ])
  })

  test('uses Ctrl/Cmd labels and hides non-display binding aliases', () => {
    const groups = getKeyboardShortcutGroups()
    const editorGroup = groups.find((group) => group.name === 'Widget editor')
    const zoomShortcut = editorGroup.shortcuts.find((shortcut) => shortcut.description === 'Canvas zoom in')

    expect(zoomShortcut.options).toEqual([
      {
        groupKey: 'mod',
        keys: ['+'],
        modifiers: ['Ctrl | CMD'],
      },
    ])
  })

  test('renders one modifier for a group of matching keys', () => {
    const groups = getKeyboardShortcutGroups()
    const synchronizationGroup = groups.find((group) => group.name === 'Timeline and sync')
    const fineClipShortcut = synchronizationGroup.shortcuts.find(
      (shortcut) => shortcut.description === 'Nudge selected clip synchronization by 0.1 seconds',
    )
    const coarseClipShortcut = synchronizationGroup.shortcuts.find(
      (shortcut) => shortcut.description === 'Nudge selected clip synchronization by 1 second',
    )

    expect(fineClipShortcut.options).toEqual([{ groupKey: '', keys: ['←', '→'], modifiers: [] }])
    expect(coarseClipShortcut.options).toEqual([{ groupKey: 'shift', keys: ['←', '→'], modifiers: ['Shift'] }])
  })

  test('does not leave an empty row when every binding is hidden', () => {
    const groups = getKeyboardShortcutGroups()
    const workspaceGroup = groups.find((group) => group.name === 'General commands')

    expect(workspaceGroup?.shortcuts.some((shortcut) => shortcut.description === 'Close widget drawer')).toBe(false)
  })

  test('shows Home and Fn+Left Arrow as separate display options', () => {
    const groups = getKeyboardShortcutGroups()
    const playbackGroup = groups.find((group) => group.name === 'Video player')
    const startShortcut = playbackGroup.shortcuts.find((shortcut) => shortcut.description === 'Rewind to start')

    expect(startShortcut.options).toEqual([
      { groupKey: 'label:Home', keys: ['Home'], modifiers: [] },
      { groupKey: 'label:Fn+Left Arrow', keys: ['Fn+Left Arrow'], modifiers: [] },
    ])
  })
})

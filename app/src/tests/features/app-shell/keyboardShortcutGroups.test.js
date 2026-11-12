import { describe, expect, test } from 'vite-plus/test'
import { getKeyboardShortcutGroups } from '@/features/app-shell/utils/keyboardShortcutGroups'

describe('keyboard shortcut help groups', () => {
  test('lists the cut, exclude and trim commands of the video player', () => {
    const playerGroup = getKeyboardShortcutGroups().find((group) => group.name === 'Video player')
    const keysOf = (description) =>
      playerGroup.shortcuts.find((shortcut) => shortcut.description === description)?.options.flatMap((option) => option.keys)

    expect(keysOf('Cut the video at the playhead')).toEqual(['C'])
    expect(keysOf('Exclude the part at the playhead from export, or include it')).toEqual(['X'])
    expect(keysOf('Trim before the playhead (the video, or the activity without videos)')).toEqual(['I'])
    expect(keysOf('Trim after the playhead (the video, or the activity without videos)')).toEqual(['O'])
  })

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
      (shortcut) => shortcut.description === 'Nudge selected clip sync offset by 0.1 seconds',
    )
    const coarseClipShortcut = synchronizationGroup.shortcuts.find(
      (shortcut) => shortcut.description === 'Nudge selected clip sync offset by 1 second',
    )

    expect(fineClipShortcut.options).toEqual([{ groupKey: '', keys: ['←', '→'], modifiers: [] }])
    expect(coarseClipShortcut.options).toEqual([{ groupKey: 'shift', keys: ['←', '→'], modifiers: ['Shift'] }])
  })

  test('does not leave an empty row when every binding is hidden', () => {
    const groups = getKeyboardShortcutGroups()
    const workspaceGroup = groups.find((group) => group.name === 'General commands')

    expect(workspaceGroup?.shortcuts.some((shortcut) => shortcut.description === 'Close widget drawer')).toBe(false)
  })

  test('shows Home as the display option for rewinding to the start', () => {
    const groups = getKeyboardShortcutGroups()
    const playbackGroup = groups.find((group) => group.name === 'Video player')
    const startShortcut = playbackGroup.shortcuts.find((shortcut) => shortcut.description === 'Rewind to start')

    expect(startShortcut.options).toEqual([{ groupKey: 'label:Home', keys: ['Home'], modifiers: [] }])
  })
})

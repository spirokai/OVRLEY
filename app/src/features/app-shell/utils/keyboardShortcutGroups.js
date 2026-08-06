import { keyboardShortcutManifest } from '@/lib/keyboard-shortcuts'

const KEY_LABELS = {
  arrowdown: '↓',
  arrowleft: '←',
  arrowright: '→',
  arrowup: '↑',
  backspace: 'Backspace',
  delete: 'Del',
  end: 'End',
  escape: 'Esc',
  home: 'Home',
  space: 'Space',
}

const MODIFIER_LABELS = {
  alt: 'Alt | OPT',
  ctrl: 'Ctrl',
  mod: 'Ctrl | CMD',
  shift: 'Shift',
}

function getBindingParts(binding) {
  if (binding.label) {
    return [binding.label, ...(binding.displayAliases || [])].map((label) => ({
      groupKey: `label:${label}`,
      keys: [label.replaceAll('Mod', 'Ctrl/⌘')],
      modifiers: [],
    }))
  }

  return [
    {
      groupKey: binding.modifiers.join('|'),
      keys: [KEY_LABELS[binding.key] || binding.key.toUpperCase()],
      modifiers: binding.modifiers.map((modifier) => MODIFIER_LABELS[modifier]),
    },
  ]
}

/**
 * Groups manifest commands for the keyboard help dialog.
 *
 * @returns {Array<{ name: string, shortcuts: Array<object> }>} Help groups.
 */
export function getKeyboardShortcutGroups() {
  const groups = []

  keyboardShortcutManifest.commands.forEach((command) => {
    if (!command.display) return
    const visibleBindings = command.bindings.filter((binding) => binding.display !== false)
    if (!visibleBindings.length) return

    let group = groups.find((candidate) => candidate.name === command.category)
    if (!group) {
      group = { name: command.category, shortcuts: [] }
      groups.push(group)
    }

    let shortcut = group.shortcuts.find((candidate) => candidate.description === command.description)
    if (!shortcut) {
      shortcut = {
        description: command.description,
        options: [],
      }
      group.shortcuts.push(shortcut)
    }

    visibleBindings.forEach((binding) => {
      getBindingParts(binding).forEach((bindingParts) => {
        const option = shortcut.options.find((candidate) => candidate.groupKey === bindingParts.groupKey)
        if (option) {
          if (!option.keys.includes(bindingParts.keys[0])) option.keys.push(bindingParts.keys[0])
          return
        }

        shortcut.options.push({
          groupKey: bindingParts.groupKey,
          keys: bindingParts.keys,
          modifiers: bindingParts.modifiers,
        })
      })
    })
  })

  return groups
}

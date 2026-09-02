import i18next from 'i18next'
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
  wheel: 'Scroll',
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
 * @param {(key: string) => string} [translate=i18next.t] - Translation function for display labels.
 * @returns {Array<{ name: string, shortcuts: Array<object> }>} Help groups.
 */
export function getKeyboardShortcutGroups(translate = i18next.t.bind(i18next)) {
  return keyboardShortcutManifest.categories
    .map((category) => {
      const shortcuts = []

      category.commands.forEach((command) => {
        if (!command.display) return
        const visibleBindings = command.bindings.filter((binding) => binding.display !== false)
        if (!visibleBindings.length) return

        visibleBindings.forEach((binding) => {
          const description = translate(binding.descriptionKey || command.descriptionKey)
          let shortcut = shortcuts.find((candidate) => candidate.description === description)
          if (!shortcut) {
            shortcut = {
              description,
              options: [],
            }
            shortcuts.push(shortcut)
          }

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

      return { name: translate(category.nameKey), shortcuts }
    })
    .filter((category) => category.shortcuts.length)
}

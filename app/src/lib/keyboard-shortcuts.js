import shortcutManifest from '@/data/keyboardShortcuts.json'

function getEventKey(event) {
  if (event.key === ' ') return 'space'
  if (event.key) return String(event.key).toLowerCase()

  const code = String(event.code || '').toLowerCase()
  return (
    {
      bracketleft: '[',
      bracketright: ']',
      digit0: '0',
      digit1: '1',
      digit2: '2',
      digit3: '3',
      equal: '=',
      minus: '-',
    }[code] || code
  )
}

function matchesModifiers(event, modifiers) {
  const usesMod = modifiers.includes('mod')

  if (usesMod && event.ctrlKey === event.metaKey) return false
  if (!usesMod && !modifiers.includes('ctrl') && (event.ctrlKey || event.metaKey)) return false
  if (modifiers.includes('ctrl') && (!event.ctrlKey || event.metaKey)) return false
  if (!usesMod && modifiers.includes('ctrl') !== Boolean(event.ctrlKey)) return false
  if (modifiers.includes('shift') !== Boolean(event.shiftKey)) return false
  if (modifiers.includes('alt') !== Boolean(event.altKey)) return false
  return true
}

/**
 * Finds the command declared for a keyboard event in one owning scope.
 *
 * @param {KeyboardEvent} event - Keyboard event to translate.
 * @param {string} scope - Shortcut owner.
 * @returns {{ commandId: string, binding: object }|null} Matching command metadata.
 */
export function matchKeyboardShortcut(event, scope) {
  const key = getEventKey(event)
  for (const command of shortcutManifest.commands) {
    if (command.scope !== scope) continue
    const binding = command.bindings.find((candidate) => candidate.key === key && matchesModifiers(event, candidate.modifiers))
    if (binding) {
      return {
        commandId: command.id,
        binding,
      }
    }
  }

  return null
}

export const keyboardShortcutManifest = shortcutManifest

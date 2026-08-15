import shortcutManifest from '@/data/keyboardShortcuts.json'

const shortcutCommands = shortcutManifest.categories.flatMap((category) => category.commands)

function getEventKey(event) {
  if (event.type === 'wheel') return 'wheel'
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
 * Finds the command declared for a keyboard or wheel event in one owning scope.
 *
 * @param {KeyboardEvent|WheelEvent} event - Event to translate.
 * @param {string} scope - Shortcut owner.
 * @returns {{ commandId: string, binding: object }|null} Matching command metadata.
 */
export function matchKeyboardShortcut(event, scope) {
  const key = getEventKey(event)
  for (const command of shortcutCommands) {
    if (command.scope !== scope) continue
    const binding = command.bindings.find(
      (candidate) => !candidate.displayOnly && candidate.key === key && matchesModifiers(event, candidate.modifiers),
    )
    if (binding) {
      return {
        commandId: command.id,
        binding,
      }
    }
  }

  return null
}

const NATIVE_EDITING_KEYS = new Set(['a', 'c', 'v', 'x', 'y', 'z'])

/**
 * Checks whether a form field should retain a matched shortcut.
 *
 * @param {KeyboardEvent} event - Keyboard event to inspect.
 * @returns {boolean} True when the shortcut should remain with the field.
 */
export function isFormFieldShortcut(event) {
  if (event.isComposing || !(event.target instanceof Element)) return Boolean(event.isComposing)

  const field = event.target.closest(
    'input, textarea, select, [role="combobox"], [role="listbox"], [role="option"], [role="slider"], [role="textbox"], [contenteditable="true"]',
  )
  if (!field) return false

  const hasCommandModifier = event.ctrlKey || event.metaKey || event.altKey
  if (!hasCommandModifier) return true

  const textEditor = event.target.closest('input, textarea, [role="textbox"], [contenteditable="true"]')
  return Boolean(textEditor && (event.ctrlKey || event.metaKey) && NATIVE_EDITING_KEYS.has(getEventKey(event)))
}

const KEYBOARD_OVERLAY_SELECTOR =
  '[data-slot="dialog-content"], [data-slot="select-content"], [data-slot="popover-content"], [data-slot="left-drawer-backdrop"]'

/**
 * Reports whether global workspace shortcuts are blocked by temporary UI.
 *
 * @returns {boolean} Whether a keyboard-owning overlay is open.
 */
export function hasOpenOverlay() {
  if (typeof document === 'undefined') return false

  return Boolean(document.querySelector(KEYBOARD_OVERLAY_SELECTOR))
}

/**
 * Reports whether Escape currently belongs to transient nested UI.
 *
 * @returns {boolean} Whether select or popover content is open.
 */
export function hasOpenPopup() {
  if (typeof document === 'undefined') return false

  return Boolean(document.querySelector('[data-slot="select-content"], [data-slot="popover-content"]'))
}

export const keyboardShortcutManifest = shortcutManifest

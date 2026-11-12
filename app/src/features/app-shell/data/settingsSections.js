import { Keyboard, Settings } from 'lucide-react'

export const SETTINGS_SECTION = 'settings'
export const SHORTCUTS_SECTION = 'shortcuts'

/**
 * Canonical settings dialog sections, in menu order. Components resolve
 * `labelKey` through i18n at render time.
 */
export const SETTINGS_SECTIONS = [
  { id: SETTINGS_SECTION, labelKey: 'app-shell.settings', icon: Settings },
  { id: SHORTCUTS_SECTION, labelKey: 'app-shell.shortcuts', icon: Keyboard },
]

/**
 * Overlay editor configuration — widget icon lookup,
 * and default preview values.
 *
 * Contains only static data (constants, lookup tables, config objects).
 * No function definitions, no side effects, no React imports beyond
 * component references used as icon lookup values.
 */

import { WIDGET_ICONS } from '@/lib/widget/widget-icons'

export { WIDGET_ICONS }

/**
 * Default activity metric values used as fallback when no real activity is loaded.
 * @type {Object<string, number|string>}
 */
export const DEFAULT_ACTIVITY_PREVIEW = {}

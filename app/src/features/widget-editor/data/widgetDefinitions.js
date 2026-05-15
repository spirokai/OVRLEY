/**
 * Widget type definitions, labels, icons, and quick-menu configuration.
 * Constants only — pure helper functions are in ../utils/widgetUtils.js.
 */

import { QUICKMENU_ITEMS, TYPE_ICONS } from '@/lib/widget-icons'

/** @type {Object<string, string>} */
export const TYPE_LABELS = {
  label: 'Text',
  speed: 'Speed',
  elevation: 'Elevation',
  heartrate: 'Heart Rate',
  power: 'Power',
  cadence: 'Cadence',
  time: 'Time',
  temperature: 'Temperature',
  gradient: 'Gradient',
  course: 'Route Map',
}

export { QUICKMENU_ITEMS, TYPE_ICONS }

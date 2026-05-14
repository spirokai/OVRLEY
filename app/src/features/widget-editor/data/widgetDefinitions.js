/**
 * Widget type definitions, labels, icons, and quick-menu configuration.
 * Constants only — pure helper functions are in ../utils/widgetUtils.js.
 */

import { Clock, Gauge, Heart, Map, Mountain, RefreshCw, Thermometer, TrendingUp, Type, Zap } from 'lucide-react'

/** @type {Array<{type: string, icon: React.ComponentType, label: string}>} */
export const QUICKMENU_ITEMS = [
  { type: 'label', icon: Type, label: 'Text' },
  { type: 'speed', icon: Gauge, label: 'Speed' },
  { type: 'elevation', icon: Mountain, label: 'Elev.' },
  { type: 'heartrate', icon: Heart, label: 'HR' },
  { type: 'power', icon: Zap, label: 'Power' },
  { type: 'cadence', icon: RefreshCw, label: 'Cadence' },
  { type: 'time', icon: Clock, label: 'Time' },
  { type: 'temperature', icon: Thermometer, label: 'Temp.' },
  { type: 'gradient', icon: TrendingUp, label: 'Grad.' },
  { type: 'course', icon: Map, label: 'Map' },
]

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

/** @type {Object<string, React.ComponentType>} */
export const TYPE_ICONS = {
  label: Type,
  speed: Gauge,
  elevation: Mountain,
  heartrate: Heart,
  power: Zap,
  cadence: RefreshCw,
  time: Clock,
  temperature: Thermometer,
  gradient: TrendingUp,
  course: Map,
}

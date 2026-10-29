/** Create widget data from declared defaults, global bindings, and activity initializers. */

import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import { DISPLAY_TYPE_DEFINITIONS, requireWidgetTypeDefinition } from './widget-catalog'
import { buildDisplayModeUpdate, initDisplayVariant } from './widget-resolver'

function applyGlobalBindings(data, bindings, globals) {
  for (const { path, source } of bindings) {
    let target = data
    for (let index = 0; index < path.length - 1; index += 1) target = target[path[index]]
    target[path.at(-1)] = globals[source]
  }
}

function initializeCourseSize(data, options, definition) {
  Object.assign(data, getCourseWidgetDimensions(options.coursePoints, definition.courseSizing))
}

function initializePointLabelFontSize(data, { sceneFontSize = data.point_label.font_size }) {
  data.point_label.font_size = sceneFontSize
}

const INITIALIZERS = { courseSizing: initializeCourseSize, pointLabelFontSize: initializePointLabelFontSize }

/**
 * @param {{type: string, displayType?: string}} selection Drawer selection, including any display-owned mode selection.
 * @param {object} globalDefaults Canonical editor defaults; omitted to use application defaults.
 * @param {{coursePoints?: number[][], sceneFontSize?: number}} options Activity and scene creation inputs.
 * @returns {object} New canonical widget data, before document identity assignment.
 */
export function createWidgetDefaults(selection, globalDefaults = DEFAULT_GLOBAL_DEFAULTS, options = {}) {
  const definition = requireWidgetTypeDefinition(selection.type)
  const { displayType = definition.defaults.display_type } = selection
  if (displayType !== definition.defaults.display_type && !definition.displayTypes.includes(displayType))
    throw new Error(`Unsupported display type: ${displayType}`)
  let data = structuredClone(definition.defaults)
  for (const initializer of definition.initializers) INITIALIZERS[initializer](data, options, definition)
  if (displayType === undefined) {
    applyGlobalBindings(data, definition.globalBindings, globalDefaults)
    return data
  }

  const display = DISPLAY_TYPE_DEFINITIONS[displayType]
  Object.assign(data, display.defaults, { display_type: displayType })
  applyGlobalBindings(data, definition.globalBindings, globalDefaults)
  if (display.modes.length > 0) Object.assign(data, buildDisplayModeUpdate(displayType, selection[display.modeSelectionField]))

  for (const type of new Set([...definition.initialDisplayTypes, displayType])) {
    data = initDisplayVariant(data, type, null)
    const variantDisplay = DISPLAY_TYPE_DEFINITIONS[type]
    const target = variantDisplay.layoutMode === 'boxed' ? data.display_variants[type] : data
    applyGlobalBindings(target, variantDisplay.globalBindings, globalDefaults)
  }
  return data
}

/**
 * @param {number[][]|null|undefined} coursePoints External activity route points.
 * @param {{maxDimension: number, minDimension: number}} sizing Manifest sizing policy.
 * @returns {{width: number, height: number}|null} Route frame, or null when no course can be measured.
 */
export function getCourseWidgetDimensions(coursePoints, sizing) {
  const validPoints = []
  let latitudeSum = 0
  for (const [latitude, longitude] of coursePoints ?? []) {
    if (Number.isFinite(latitude) && Number.isFinite(longitude)) {
      validPoints.push([latitude, longitude])
      latitudeSum += latitude
    }
  }
  if (validPoints.length < 2) return null
  const meanLatitudeRadians = (latitudeSum / validPoints.length) * (Math.PI / 180)
  const projectedX = []
  const projectedY = []
  for (const [latitude, longitude] of validPoints) {
    projectedX.push(longitude * Math.cos(meanLatitudeRadians))
    projectedY.push(latitude)
  }
  const spanX = Math.max(Math.max(...projectedX) - Math.min(...projectedX), 1e-6)
  const spanY = Math.max(Math.max(...projectedY) - Math.min(...projectedY), 1e-6)
  const factor = sizing.maxDimension / Math.max(spanX, spanY)
  return { width: Math.max(Math.round(spanX * factor), sizing.minDimension), height: Math.max(Math.round(spanY * factor), sizing.minDimension) }
}

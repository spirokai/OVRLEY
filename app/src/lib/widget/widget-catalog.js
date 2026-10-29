/** Resolve manifest inheritance once; consumers use the resulting widget and display contracts. */

import widgetsManifest from '../../../../assets/standard-widgets.json'
import metricsManifest from '../../../../assets/standard-metrics.json'

function compileBindings(bindings) {
  const compiled = []
  for (const [field, source] of Object.entries(bindings)) compiled.push({ path: field.split('.'), source })
  return compiled
}

function compileDisplayDefinitions() {
  const definitions = {}
  for (const manifest of [widgetsManifest, metricsManifest]) {
    for (const [type, raw] of Object.entries(manifest.displayTypes.definitions)) {
      const definition = { ...widgetsManifest.displayDefaults, ...manifest.displayDefaults, ...raw }
      const modes = []
      const modesByValue = {}
      for (const mode of definition.modes) {
        const defaults = { ...definition.modeDefaults, ...mode.defaults }
        for (const [target, source] of Object.entries(definition.modeFields)) defaults[target] = mode[source]
        const compiled = { ...mode, defaults }
        modes.push(compiled)
        modesByValue[mode.value] = compiled
      }
      definitions[type] = Object.freeze({
        ...definition,
        defaults: { ...definition.defaults, ...definition.modeDefaults },
        variantKeys: [...Object.keys(definition.variantDefaults), ...definition.conditionalKeys],
        geometryKeys: Object.keys(definition.geometryBindings),
        globalBindings: compileBindings(definition.globalBindings),
        modes,
        modesByValue,
      })
    }
  }
  return Object.freeze(definitions)
}

/** Ordered document collections and their type discriminators. */
export const WIDGET_COLLECTIONS = widgetsManifest.collections
/** Ordered drawer categories. */
export const WIDGET_CATEGORY_NAME_KEYS = widgetsManifest.categories
/** Unit scales used by altitude input and presentation. */
export const ALTITUDE_UNIT_SCALES = widgetsManifest.altitudeUnitScales
/** Defaults, geometry, modes, and optional fields for each display. */
export const DISPLAY_TYPE_DEFINITIONS = compileDisplayDefinitions()

function compileWidgetDefinitions() {
  const definitions = {}
  for (const manifest of [widgetsManifest, metricsManifest]) {
    for (const raw of manifest.definitions) {
      const definition = { ...widgetsManifest.widgetDefaults, ...manifest.widgetDefaults, ...raw }
      const preset = widgetsManifest.dataDefaults[definition.defaultsPreset]
      const displayType = definition.defaults.display_type ?? preset.display_type
      const defaults = { ...preset, ...DISPLAY_TYPE_DEFINITIONS[displayType]?.defaults, ...definition.defaults }
      const collection = WIDGET_COLLECTIONS[definition.collection]
      if (collection.typeField !== null) defaults[collection.typeField] = definition.type
      const configKeys = new Set(['id', ...Object.keys(defaults), ...definition.optionalFields])
      for (const type of definition.displayTypes) {
        for (const field of Object.keys(DISPLAY_TYPE_DEFINITIONS[type].modeFields)) configKeys.add(field)
      }
      definitions[definition.type] = Object.freeze({
        ...definition,
        defaults,
        globalBindings: compileBindings(definition.globalBindings),
        configKeys: [...configKeys],
        hasDecimalControl: definition.maxDecimals !== undefined || definition.formatter === 'decimal' || definition.formatter === 'temperature',
        maxDecimals: definition.maxDecimals ?? 1,
      })
    }
  }
  return Object.freeze(definitions)
}

/** Canonical defaults and capabilities for every widget. */
export const WIDGET_TYPE_DEFINITIONS = compileWidgetDefinitions()

/** @param {string} type Widget type selected at ingress. @returns {object} Required widget definition. */
export function requireWidgetTypeDefinition(type) {
  if (!Object.hasOwn(WIDGET_TYPE_DEFINITIONS, type)) throw new Error(`Unknown widget type: ${type}`)
  return WIDGET_TYPE_DEFINITIONS[type]
}

/** @param {string} displayType Active display. @param {string} value Selected mode. @returns {object} Mode metadata and defaults. */
export function requireDisplayMode(displayType, value) {
  const modes = DISPLAY_TYPE_DEFINITIONS[displayType].modesByValue
  if (!Object.hasOwn(modes, value)) throw new Error(`Unsupported ${displayType} mode: ${value}`)
  return modes[value]
}

/**
 * A widget may name a shared display after its own metric (e.g. depth's "Depth Tape").
 * @param {string} type Widget type. @param {string} displayType One of its displays. @returns {string} Translation key for the display's name.
 */
export function getDisplayTypeLabelKey(type, displayType) {
  return requireWidgetTypeDefinition(type).displayTypeLabelKeys[displayType] ?? DISPLAY_TYPE_DEFINITIONS[displayType].labelKey
}

/** @param {string} type Widget type. @param {Function} translate Translator. @returns {object[]} Inspector display choices. */
export function getWidgetDisplayTypeOptions(type, translate) {
  const options = []
  for (const value of requireWidgetTypeDefinition(type).displayTypes) options.push({ value, label: translate(getDisplayTypeLabelKey(type, value)) })
  return options
}

/** @param {object} widget Canonical config widget. @returns {boolean} Whether its active presentation has a bounded frame. */
export function isFramedWidget(widget) {
  const display = widget.data.display_type
  const definition = display === undefined ? WIDGET_TYPE_DEFINITIONS[widget.type] : DISPLAY_TYPE_DEFINITIONS[display]
  return definition.layoutMode === 'boxed'
}

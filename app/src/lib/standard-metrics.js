import standardMetricsManifest from '../../../assets/standard-metrics.json'

const STANDARD_METRIC_DEFINITIONS = Object.fromEntries(standardMetricsManifest.definitions.map((definition) => [definition.type, definition]))

const CURRENT_STANDARD_METRIC_WIDGET_TYPES = standardMetricsManifest.definitions
  .filter((definition) => definition.current)
  .map((definition) => definition.type)

export { CURRENT_STANDARD_METRIC_WIDGET_TYPES }

export const STANDARD_METRIC_WIDGET_TYPES = standardMetricsManifest.definitions.map((definition) => definition.type)

export function isStandardMetricWidgetType(type) {
  return Object.hasOwn(STANDARD_METRIC_DEFINITIONS, type)
}

export function getStandardMetricDefinition(type) {
  return STANDARD_METRIC_DEFINITIONS[type] ?? null
}

export function getStandardMetricDisplayUnit(type, widgetData = {}) {
  const definition = getStandardMetricDefinition(type)
  return widgetData.display_unit || definition?.defaultDisplayUnit || null
}

export function getStandardMetricUnitOptions(type) {
  return getStandardMetricDefinition(type)?.supportedDisplayUnits ?? []
}

export function getStandardMetricUnitLabel(type, displayUnit) {
  const definition = getStandardMetricDefinition(type)
  const resolvedUnit = displayUnit || definition?.defaultDisplayUnit
  const option = definition?.supportedDisplayUnits.find((candidate) => candidate.value === resolvedUnit)
  return option?.renderLabel ?? option?.label ?? ''
}

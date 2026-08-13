import { convertStandardMetricValue } from '@/features/widget-preview/widgets/metric/format'
import { getElevationProfileSeries, getInterpolatedActivityValue, getMetricSeries } from '@/features/overlay-editor/utils/overlayEditorUtils'

/** Returns the first present altitude sample in a presentation series. */
export function getFirstAltitudeValue(series) {
  return series.find((value) => value !== null && value !== undefined) ?? null
}

/** Converts an altitude value between the two supported display units. */
export function convertAltitudeValue(value, fromUnit, toUnit) {
  if (fromUnit === toUnit) return value
  const fromScale = convertStandardMetricValue('altitude', 1, fromUnit)
  const toScale = convertStandardMetricValue('altitude', 1, toUnit)
  return (value / fromScale) * toScale
}

/** Converts and rounds a configured altitude input while preserving an empty value. */
export function convertAltitudeInputValue(value, fromUnit, toUnit) {
  if (value === null || value === undefined || value === '') return value
  const convertedAltitude = convertAltitudeValue(value, fromUnit, toUnit)
  return Math.round(convertedAltitude)
}

/** Resolves the activity-backed initial value shown by an altitude editor. */
export function getInitialAltitudeValue(type, activity, unit = 'm') {
  if (!activity) return undefined
  const series = type === 'altitude' ? getMetricSeries(activity, type) : getElevationProfileSeries(activity)
  const firstValue = getFirstAltitudeValue(series ?? [])
  return firstValue === null ? undefined : Math.round(convertAltitudeValue(firstValue, 'm', unit))
}

/** Materializes an absent altitude target for editor presentation only. */
export function resolveInitialAltitudePresentation(widget, activity) {
  if (!['altitude', 'elevation'].includes(widget.type) || (widget.data.starting_altitude !== null && widget.data.starting_altitude !== undefined)) {
    return widget
  }
  const unit = widget.type === 'altitude' ? widget.data.display_unit : widget.data.starting_altitude_unit
  const startingAltitude = getInitialAltitudeValue(widget.type, activity, unit)
  return startingAltitude === undefined ? widget : { ...widget, data: { ...widget.data, starting_altitude: startingAltitude } }
}

/** Builds a unit update while preserving an altitude widget's physical starting value. */
export function buildMetricUnitUpdate(metricType, startingAltitude, currentUnit, nextUnit) {
  const update = { display_unit: nextUnit }
  if (metricType === 'altitude') {
    update.starting_altitude = convertAltitudeInputValue(startingAltitude, currentUnit, nextUnit)
  }
  return update
}

/** Resolves the constant meter offset needed to make the first sample equal the configured target. */
export function getAltitudeCorrectionMeters(series, startingAltitude, unit) {
  if (startingAltitude === null || startingAltitude === undefined || startingAltitude === '') return 0
  const firstValue = getFirstAltitudeValue(series)
  if (firstValue === null) return 0
  return convertAltitudeValue(startingAltitude, unit, 'm') - firstValue
}

/** Applies a resolved presentation offset to one altitude value. */
export function applyAltitudeOffset(value, offset) {
  if (value === null || value === undefined) return value
  return value + offset
}

/** Resolves canonical geometry inputs and corrected display values for a metric widget. */
export function resolveMetricPresentationValues(widget, activity, previewSecond) {
  const values = getMetricSeries(activity, widget.type) ?? []
  const rawValue = getInterpolatedActivityValue(activity, widget.type, previewSecond)

  if (widget.type !== 'altitude') {
    return { rawValue, values, value: rawValue, valueOffset: 0 }
  }

  const valueOffset = getAltitudeCorrectionMeters(values, widget.data.starting_altitude, widget.data.display_unit)
  return {
    rawValue,
    values,
    value: applyAltitudeOffset(rawValue, valueOffset),
    valueOffset,
  }
}

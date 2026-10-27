import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'

const { chartLimits, seriesPalette } = WIDGET_TYPE_DEFINITIONS.rolling_chart

/** Bounds of the series list and time window shared with the renderer. */
export const CHART_LIMITS = chartLimits

/** Metrics a rolling chart may plot, in catalog order. */
export const PLOTTABLE_METRICS = Object.values(WIDGET_TYPE_DEFINITIONS)
  .filter((definition) => definition.plottable)
  .map((definition) => definition.type)

/**
 * Parses whole seconds ("45") or minutes and seconds ("1:30").
 *
 * @param {string} text - Typed window length.
 * @returns {number|null} Seconds within the chart limits, or null when malformed or out of range.
 */
export function parseWindowSeconds(text) {
  const match = /^\s*(?:(\d+):([0-5]\d)|(\d+))\s*$/.exec(text)
  if (!match) return null
  const seconds = match[3] === undefined ? Number(match[1]) * 60 + Number(match[2]) : Number(match[3])
  return seconds >= CHART_LIMITS.minWindowSeconds && seconds <= CHART_LIMITS.maxWindowSeconds ? seconds : null
}

/**
 * @param {number} seconds - Window length.
 * @returns {string} The length as "m:ss".
 */
export function formatWindowSeconds(seconds) {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/**
 * @param {object[]} series - Current series rows.
 * @returns {string[]} Plottable metrics not yet plotted.
 */
export function unusedChartMetrics(series) {
  const used = new Set(series.map((item) => item.metric))
  return PLOTTABLE_METRICS.filter((metric) => !used.has(metric))
}

/**
 * Appends the first unplotted metric in the first palette color no row uses yet.
 *
 * @param {object[]} series - Current series rows; below the series limit with a metric left to add.
 * @returns {object[]} Next series rows.
 */
export function addChartSeries(series) {
  const metric = unusedChartMetrics(series)[0]
  const usedColors = new Set(series.map((item) => item.color))
  const color = seriesPalette.find((candidate) => !usedColors.has(candidate)) ?? seriesPalette[series.length % seriesPalette.length]
  return [...series, { metric, color }]
}

/**
 * @param {object[]} series - Current series rows.
 * @param {number} index - Edited row.
 * @param {string} metric - Newly picked metric.
 * @returns {object[]} Next series rows.
 */
export function replaceChartSeriesMetric(series, index, metric) {
  return series.map((item, itemIndex) => (itemIndex === index ? { ...item, metric } : item))
}

/**
 * @param {object[]} series - Current series rows.
 * @param {number} index - Edited row.
 * @param {string} color - Hex color.
 * @returns {object[]} Next series rows.
 */
export function recolorChartSeries(series, index, color) {
  return series.map((item, itemIndex) => (itemIndex === index ? { ...item, color } : item))
}

/**
 * Swaps a row with its neighbour; rows draw back to front.
 *
 * @param {object[]} series - Current series rows.
 * @param {number} index - Moved row.
 * @param {-1|1} offset - Direction; the neighbour must exist.
 * @returns {object[]} Next series rows.
 */
export function moveChartSeries(series, index, offset) {
  const next = [...series]
  ;[next[index], next[index + offset]] = [next[index + offset], next[index]]
  return next
}

/**
 * @param {object[]} series - Current series rows; more than one.
 * @param {number} index - Removed row.
 * @returns {object[]} Next series rows.
 */
export function removeChartSeries(series, index) {
  return series.filter((_, itemIndex) => itemIndex !== index)
}

/**
 * Provides overlay editor helpers for metric widget assets.
 */

import cadenceIconSvg from '../widgets/icons/widget-cadence.svg?raw'
import heartrateIconSvg from '../widgets/icons/widget-heartrate.svg?raw'
import powerIconSvg from '../widgets/icons/widget-power.svg?raw'
import speedIconSvg from '../widgets/icons/widget-speed.svg?raw'
import temperatureIconSvg from '../widgets/icons/widget-temperature.svg?raw'
import timeIconSvg from '../widgets/icons/widget-time.svg?raw'

/**
 * Parses icon svg markup into reusable preview data.
 *
 * @param {*} svgMarkup - Value for svg markup.
 * @returns {object} Derived data structure for downstream use.
 */
function parseMetricIconSvg(svgMarkup) {
  const strokeWidthMatch = svgMarkup.match(/stroke-width="([^"]+)"/)
  const innerMarkupMatch = svgMarkup.match(/<svg[^>]*>([\s\S]*?)<\/svg>/i)

  return {
    strokeWidth: Number(strokeWidthMatch?.[1] || 2),
    innerMarkup: (innerMarkupMatch?.[1] || '').trim(),
  }
}

export const METRIC_ICON_SVGS = {
  cadence: parseMetricIconSvg(cadenceIconSvg),
  heartrate: parseMetricIconSvg(heartrateIconSvg),
  power: parseMetricIconSvg(powerIconSvg),
  speed: parseMetricIconSvg(speedIconSvg),
  temperature: parseMetricIconSvg(temperatureIconSvg),
  time: parseMetricIconSvg(timeIconSvg),
}

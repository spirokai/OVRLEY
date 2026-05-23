import cadenceIconSvg from '../../../assets/widget-icons/widget-cadence.svg?raw'
import heartrateIconSvg from '../../../assets/widget-icons/widget-heartrate.svg?raw'
import powerIconSvg from '../../../assets/widget-icons/widget-power.svg?raw'
import speedIconSvg from '../../../assets/widget-icons/widget-speed.svg?raw'
import temperatureIconSvg from '../../../assets/widget-icons/widget-temperature.svg?raw'
import timeIconSvg from '../../../assets/widget-icons/widget-time.svg?raw'
import gradientIconSvg from '../components/widgets/icons/widget-gradient.svg?raw'
import courseIconSvg from '../components/widgets/icons/widget-course.svg?raw'
import elevationIconSvg from '../components/widgets/icons/widget-elevation.svg?raw'
import labelIconSvg from '../components/widgets/icons/widget-label.svg?raw'

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
  gradient: parseMetricIconSvg(gradientIconSvg),
  course: parseMetricIconSvg(courseIconSvg),
  elevation: parseMetricIconSvg(elevationIconSvg),
  label: parseMetricIconSvg(labelIconSvg),
}

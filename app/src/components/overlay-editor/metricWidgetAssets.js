import cadenceIconSvg from '../widgets/icons/widget-cadence.svg?raw'
import heartrateIconSvg from '../widgets/icons/widget-heartrate.svg?raw'
import powerIconSvg from '../widgets/icons/widget-power.svg?raw'
import speedIconSvg from '../widgets/icons/widget-speed.svg?raw'
import temperatureIconSvg from '../widgets/icons/widget-temperature.svg?raw'
import timeIconSvg from '../widgets/icons/widget-time.svg?raw'

function normalizeIconSvg(svgMarkup) {
  return svgMarkup.replace(
    '<svg ',
    '<svg width="100%" height="100%" focusable="false" aria-hidden="true" preserveAspectRatio="xMidYMid meet" ',
  )
}

export const METRIC_ICON_SVGS = {
  cadence: normalizeIconSvg(cadenceIconSvg),
  heartrate: normalizeIconSvg(heartrateIconSvg),
  power: normalizeIconSvg(powerIconSvg),
  speed: normalizeIconSvg(speedIconSvg),
  temperature: normalizeIconSvg(temperatureIconSvg),
  time: normalizeIconSvg(timeIconSvg),
}

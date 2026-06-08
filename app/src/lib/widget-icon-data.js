import airPressureIconSvg from '../../../assets/widget-icons/widget-air-pressure.svg?raw'
import cadenceIconSvg from '../../../assets/widget-icons/widget-cadence.svg?raw'
import coreTemperatureIconSvg from '../../../assets/widget-icons/widget-core-temperature.svg?raw'
import gearPositionIconSvg from '../../../assets/widget-icons/widget-gear-position.svg?raw'
import gForceIconSvg from '../../../assets/widget-icons/widget-g-force.svg?raw'
import groundContactTimeIconSvg from '../../../assets/widget-icons/widget-ground-contact-time.svg?raw'
import heartrateIconSvg from '../../../assets/widget-icons/widget-heartrate.svg?raw'
import leftRightBalanceIconSvg from '../../../assets/widget-icons/widget-left-right-balance.svg?raw'
import paceIconSvg from '../../../assets/widget-icons/widget-pace.svg?raw'
import powerIconSvg from '../../../assets/widget-icons/widget-power.svg?raw'
import speedIconSvg from '../../../assets/widget-icons/widget-speed.svg?raw'
import strideLengthIconSvg from '../../../assets/widget-icons/widget-stride-length.svg?raw'
import strokeRateIconSvg from '../../../assets/widget-icons/widget-stroke-rate.svg?raw'
import temperatureIconSvg from '../../../assets/widget-icons/widget-temperature.svg?raw'
import timeIconSvg from '../../../assets/widget-icons/widget-time.svg?raw'
import torqueIconSvg from '../../../assets/widget-icons/widget-torque.svg?raw'
import verticalOscillationIconSvg from '../../../assets/widget-icons/widget-vertical-oscillation.svg?raw'
import verticalRatioIconSvg from '../../../assets/widget-icons/widget-vertical-ratio.svg?raw'
import verticalSpeedIconSvg from '../../../assets/widget-icons/widget-vertical-speed.svg?raw'
import gradientIconSvg from '@/components/widgets/icons/widget-gradient.svg?raw'
import courseIconSvg from '@/components/widgets/icons/widget-course.svg?raw'
import elevationIconSvg from '@/components/widgets/icons/widget-elevation.svg?raw'
import labelIconSvg from '@/components/widgets/icons/widget-label.svg?raw'
import headingIconSvg from '../../../assets/widget-icons/widget-heading.svg?raw'
import altitudeIconSvg from '../../../assets/widget-icons/widget-altitude.svg?raw'
import isoIconSvg from '../../../assets/widget-icons/widget-iso.svg?raw'
import apertureIconSvg from '../../../assets/widget-icons/widget-aperture.svg?raw'
import shutterSpeedIconSvg from '../../../assets/widget-icons/widget-shutter-speed.svg?raw'
import focalLengthIconSvg from '../../../assets/widget-icons/widget-focal-length.svg?raw'
import evIconSvg from '../../../assets/widget-icons/widget-ev.svg?raw'
import colorTemperatureIconSvg from '../../../assets/widget-icons/widget-color-temperature.svg?raw'

function parseMetricIconSvg(svgMarkup) {
  const strokeWidthMatch = svgMarkup.match(/stroke-width="([^"]+)"/)
  const innerMarkupMatch = svgMarkup.match(/<svg[^>]*>([\s\S]*?)<\/svg>/i)
  return {
    strokeWidth: Number(strokeWidthMatch?.[1] || 2),
    innerMarkup: (innerMarkupMatch?.[1] || '').trim(),
  }
}

export const METRIC_ICON_SVGS = {
  air_pressure: parseMetricIconSvg(airPressureIconSvg),
  cadence: parseMetricIconSvg(cadenceIconSvg),
  core_temperature: parseMetricIconSvg(coreTemperatureIconSvg),
  gear_position: parseMetricIconSvg(gearPositionIconSvg),
  g_force: parseMetricIconSvg(gForceIconSvg),
  ground_contact_time: parseMetricIconSvg(groundContactTimeIconSvg),
  heartrate: parseMetricIconSvg(heartrateIconSvg),
  left_right_balance: parseMetricIconSvg(leftRightBalanceIconSvg),
  pace: parseMetricIconSvg(paceIconSvg),
  power: parseMetricIconSvg(powerIconSvg),
  speed: parseMetricIconSvg(speedIconSvg),
  stride_length: parseMetricIconSvg(strideLengthIconSvg),
  stroke_rate: parseMetricIconSvg(strokeRateIconSvg),
  temperature: parseMetricIconSvg(temperatureIconSvg),
  time: parseMetricIconSvg(timeIconSvg),
  torque: parseMetricIconSvg(torqueIconSvg),
  vertical_oscillation: parseMetricIconSvg(verticalOscillationIconSvg),
  vertical_ratio: parseMetricIconSvg(verticalRatioIconSvg),
  vertical_speed: parseMetricIconSvg(verticalSpeedIconSvg),
  gradient: parseMetricIconSvg(gradientIconSvg),
  course: parseMetricIconSvg(courseIconSvg),
  elevation: parseMetricIconSvg(elevationIconSvg),
  heading: parseMetricIconSvg(headingIconSvg),
  altitude: parseMetricIconSvg(altitudeIconSvg),
  iso: parseMetricIconSvg(isoIconSvg),
  aperture: parseMetricIconSvg(apertureIconSvg),
  shutter_speed: parseMetricIconSvg(shutterSpeedIconSvg),
  focal_length: parseMetricIconSvg(focalLengthIconSvg),
  ev: parseMetricIconSvg(evIconSvg),
  color_temperature: parseMetricIconSvg(colorTemperatureIconSvg),
  label: parseMetricIconSvg(labelIconSvg),
}

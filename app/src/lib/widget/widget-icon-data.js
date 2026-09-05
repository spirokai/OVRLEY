import airPressureIconSvg from '../../../../assets/widget-icons/widget-air-pressure.svg?raw'
import cadenceIconSvg from '../../../../assets/widget-icons/widget-cadence.svg?raw'
import coreTemperatureIconSvg from '../../../../assets/widget-icons/widget-core-temperature.svg?raw'
import distanceIconSvg from '../../../../assets/widget-icons/widget-distance.svg?raw'
import gearPositionIconSvg from '../../../../assets/widget-icons/widget-gear-position.svg?raw'
import gForceIconSvg from '../../../../assets/widget-icons/widget-g-force.svg?raw'
import groundContactTimeIconSvg from '../../../../assets/widget-icons/widget-ground-contact-time.svg?raw'
import heartrateIconSvg from '../../../../assets/widget-icons/widget-heartrate.svg?raw'
import leftRightBalanceIconSvg from '../../../../assets/widget-icons/widget-left-right-balance.svg?raw'
import paceIconSvg from '../../../../assets/widget-icons/widget-pace.svg?raw'
import powerIconSvg from '../../../../assets/widget-icons/widget-power.svg?raw'
import speedIconSvg from '../../../../assets/widget-icons/widget-speed.svg?raw'
import strideLengthIconSvg from '../../../../assets/widget-icons/widget-stride-length.svg?raw'
import strokeRateIconSvg from '../../../../assets/widget-icons/widget-stroke-rate.svg?raw'
import temperatureIconSvg from '../../../../assets/widget-icons/widget-temperature.svg?raw'
import timeIconSvg from '../../../../assets/widget-icons/widget-time.svg?raw'
import torqueIconSvg from '../../../../assets/widget-icons/widget-torque.svg?raw'
import verticalOscillationIconSvg from '../../../../assets/widget-icons/widget-vertical-oscillation.svg?raw'
import verticalRatioIconSvg from '../../../../assets/widget-icons/widget-vertical-ratio.svg?raw'
import verticalSpeedIconSvg from '../../../../assets/widget-icons/widget-vertical-speed.svg?raw'
import gradientIconSvg from '@/components/widgets/icons/widget-gradient.svg?raw'
import courseIconSvg from '@/components/widgets/icons/widget-course.svg?raw'
import elevationIconSvg from '@/components/widgets/icons/widget-elevation.svg?raw'
import labelIconSvg from '@/components/widgets/icons/widget-label.svg?raw'
import headingIconSvg from '../../../../assets/widget-icons/widget-heading.svg?raw'
import altitudeIconSvg from '../../../../assets/widget-icons/widget-altitude.svg?raw'
import isoIconSvg from '../../../../assets/widget-icons/widget-iso.svg?raw'
import apertureIconSvg from '../../../../assets/widget-icons/widget-aperture.svg?raw'
import shutterSpeedIconSvg from '../../../../assets/widget-icons/widget-shutter-speed.svg?raw'
import focalLengthIconSvg from '../../../../assets/widget-icons/widget-focal-length.svg?raw'
import evIconSvg from '../../../../assets/widget-icons/widget-ev.svg?raw'
import colorTemperatureIconSvg from '../../../../assets/widget-icons/widget-color-temperature.svg?raw'
import rpmIconSvg from '../../../../assets/widget-icons/widget-rpm.svg?raw'
import throttlePositionIconSvg from '../../../../assets/widget-icons/widget-throttle-position.svg?raw'
import brakePositionIconSvg from '../../../../assets/widget-icons/widget-brake-position.svg?raw'
import leanAngleIconSvg from '../../../../assets/widget-icons/widget-lean-angle.svg?raw'
import houseIconSvg from '../../../../assets/widget-icons/widget-house.svg?raw'
import satelliteIconSvg from '../../../../assets/widget-icons/widget-satellite.svg?raw'
import arrowUpNarrowWideIconSvg from '../../../../assets/widget-icons/widget-arrow-up-narrow-wide.svg?raw'
import caloriesIconSvg from '../../../../assets/widget-icons/widget-calories.svg?raw'
import displayTypeTextSvg from '../../../../assets/widget-icons/display-type-text.svg?raw'
import displayTypeLinearSvg from '../../../../assets/widget-icons/display-type-linear.svg?raw'
import displayTypeHeadingTapeSvg from '../../../../assets/widget-icons/display-type-heading-tape.svg?raw'
import displayTypeLeanAngleSvg from '../../../../assets/widget-icons/display-type-lean-angle.svg?raw'
import displayTypeArcSvg from '../../../../assets/widget-icons/display-type-arc.svg?raw'
import displayTypeCornerSvg from '../../../../assets/widget-icons/display-type-corner.svg?raw'
import displayTypeCircleSvg from '../../../../assets/widget-icons/display-type-circle.svg?raw'
import displayTypeRectangleSvg from '../../../../assets/widget-icons/display-type-rectangle.svg?raw'
import displayTypeGForceSvg from '../../../../assets/widget-icons/display-type-g-force.svg?raw'
import engineLoadIconSvg from '../../../../assets/widget-icons/widget-engine-load.svg?raw'

const iconAssetModules = import.meta.glob('../../../../assets/widget-icons/*.svg', {
  eager: true,
  import: 'default',
  query: '?raw',
})

const iconSvgMarkupByAssetFile = Object.fromEntries(
  Object.entries(iconAssetModules).map(([path, svgMarkup]) => [path.slice(path.lastIndexOf('/') + 1), svgMarkup]),
)
const parsedIconAssets = new Map()

function parseMetricIconSvg(svgMarkup) {
  const rootTag = svgMarkup.match(/<svg[^>]*>/i)?.[0]
  if (!rootTag) throw new Error('Metric icon asset must contain an SVG root element')

  const strokeWidthMatch = svgMarkup.match(/stroke-width="([^"]+)"/)
  const innerMarkupMatch = svgMarkup.match(/<svg[^>]*>([\s\S]*?)<\/svg>/i)
  const fill = rootTag.match(/\sfill="([^"]+)"/)?.[1]
  const stroke = rootTag.match(/\sstroke="([^"]+)"/)?.[1]
  if (!fill || !stroke) throw new Error('Metric icon SVG root must define fill and stroke')

  return {
    fill,
    stroke,
    strokeWidth: Number(strokeWidthMatch?.[1] || 2),
    innerMarkup: (innerMarkupMatch?.[1] || '').trim(),
  }
}

/**
 * Resolves an SVG asset discovered from `assets/widget-icons` by its manifest filename.
 * @param {string} assetFile - Exact `assetFile` value from a manifest icon definition.
 * @returns {{fill: string, stroke: string, strokeWidth: number, innerMarkup: string}} Parsed SVG icon data.
 */
export function getIconSvgByAssetFile(assetFile) {
  if (!Object.hasOwn(iconSvgMarkupByAssetFile, assetFile)) throw new Error(`Unsupported icon asset: ${assetFile}`)
  if (!parsedIconAssets.has(assetFile)) parsedIconAssets.set(assetFile, parseMetricIconSvg(iconSvgMarkupByAssetFile[assetFile]))
  return parsedIconAssets.get(assetFile)
}

export const METRIC_ICON_SVGS = {
  air_pressure: parseMetricIconSvg(airPressureIconSvg),
  cadence: parseMetricIconSvg(cadenceIconSvg),
  core_temperature: parseMetricIconSvg(coreTemperatureIconSvg),
  distance: parseMetricIconSvg(distanceIconSvg),
  gear_position: parseMetricIconSvg(gearPositionIconSvg),
  g_force: parseMetricIconSvg(gForceIconSvg),
  ground_contact_time: parseMetricIconSvg(groundContactTimeIconSvg),
  heartrate: parseMetricIconSvg(heartrateIconSvg),
  left_right_balance: parseMetricIconSvg(leftRightBalanceIconSvg),
  pace: parseMetricIconSvg(paceIconSvg),
  power: parseMetricIconSvg(powerIconSvg),
  engine_power: parseMetricIconSvg(powerIconSvg),
  engine_load: parseMetricIconSvg(engineLoadIconSvg),
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
  rpm: parseMetricIconSvg(rpmIconSvg),
  throttle_position: parseMetricIconSvg(throttlePositionIconSvg),
  brake_position: parseMetricIconSvg(brakePositionIconSvg),
  lean_angle: parseMetricIconSvg(leanAngleIconSvg),
  gps_coordinates: parseMetricIconSvg(satelliteIconSvg),
  distance_to_home: parseMetricIconSvg(houseIconSvg),
  total_ascent: parseMetricIconSvg(arrowUpNarrowWideIconSvg),
  calories: parseMetricIconSvg(caloriesIconSvg),
  label: parseMetricIconSvg(labelIconSvg),
}

export const DISPLAY_TYPE_ICON_SVGS = {
  text: parseMetricIconSvg(displayTypeTextSvg),
  linear: parseMetricIconSvg(displayTypeLinearSvg),
  heading_tape: parseMetricIconSvg(displayTypeHeadingTapeSvg),
  lean_angle: parseMetricIconSvg(displayTypeLeanAngleSvg),
  arc: parseMetricIconSvg(displayTypeArcSvg),
  corner: parseMetricIconSvg(displayTypeCornerSvg),
  circle: parseMetricIconSvg(displayTypeCircleSvg),
  rectangle: parseMetricIconSvg(displayTypeRectangleSvg),
  g_force: parseMetricIconSvg(displayTypeGForceSvg),
}

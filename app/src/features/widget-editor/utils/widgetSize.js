/**
 * Inspector size controls scale their editable content and preserve display variants.
 * Canvas gestures use Rust's resize policy.
 */

import { clamp } from '@/lib/utils'
import { resolveActiveWidgetData } from '@/lib/widget/widget-resolver'

const GAUGE_SIZE_FIELDS = [
  ['track_border_thickness', 0, 24],
  ['inner_widget_offset_x', -10_000, 10_000],
  ['inner_widget_offset_y', -10_000, 10_000],
  ['min_max_label_font_size', 6, 50],
]
const LEAN_SIZE_FIELDS = [
  ['value_offset_x', -10_000, 10_000],
  ['value_offset_y', -10_000, 10_000],
]
const COMPASS_SIZE_FIELDS = [
  ['cardinal_font_size', 4, 400],
  ['indicator_size', 1, 10_000],
  ['value_offset_x', -10_000, 10_000],
  ['value_offset_y', -10_000, 10_000],
]
const G_FORCE_SIZE_FIELDS = [
  ['label_font_size', 8, 400],
  ['label_offset_x', -10_000, 10_000],
  ['label_offset_y', -10_000, 10_000],
]

function scaleNumber(value, factor, min, max) {
  return clamp(Math.round(value * factor), min, max)
}

function scaleFields(data, factor, fields) {
  const patch = {}
  for (const [key, min, max] of fields) patch[key] = scaleNumber(data[key], factor, min, max)
  return patch
}

function buildGaugeSize(data, factor, displayType) {
  const thickness = scaleNumber(data.track_thickness, factor, 1, 100)
  return {
    ...scaleFields(data, factor, GAUGE_SIZE_FIELDS),
    ...(displayType === 'gt_gauge' ? { tick_length: scaleNumber(data.tick_length, factor, 0, 100) } : {}),
    track_thickness: thickness,
    track_corner_radius: scaleNumber(data.track_corner_radius, factor, 0, thickness * 0.5),
  }
}

function buildLeanSize(data, factor) {
  const diameter = scaleNumber(data.diameter, factor, 8, 10_000)
  const thickness = scaleNumber(data.track_thickness, factor, 1, diameter * 0.5 - Number.EPSILON)
  return {
    ...scaleFields(data, factor, LEAN_SIZE_FIELDS),
    diameter,
    track_thickness: thickness,
    track_border_thickness: scaleNumber(data.track_border_thickness, factor, 0, Math.max(0, thickness * 0.5 - Number.EPSILON)),
  }
}

function buildCompassSize(data, factor) {
  return {
    ...scaleFields(data, factor, COMPASS_SIZE_FIELDS),
    diameter: scaleNumber(data.diameter, factor, 8, 10_000),
    track_thickness: scaleNumber(data.track_thickness, factor, 1, 10_000),
    track_corner_radius: scaleNumber(data.track_corner_radius, factor, 0, 10_000),
  }
}

function buildGForceSize(data, factor) {
  const diameter = scaleNumber(data.diameter, factor, 8, 10_000)
  return {
    ...scaleFields(data, factor, G_FORCE_SIZE_FIELDS),
    diameter,
    border_thickness: scaleNumber(data.border_thickness, factor, 0, (diameter - 1) * 0.5),
    marker_size: scaleNumber(data.marker_size, factor, 1, diameter),
  }
}

/** @param {object} widget Resolved gauge, lean-angle, heading-compass, or G-force widget. @param {number} size Inspector size in widget pixels. @returns {object} Canonical data patch. */
export function buildUniformResizeUpdate(widget, size) {
  const displayType = widget.data.display_type
  const data = resolveActiveWidgetData(widget.data)
  const variant = widget.data.display_variants[displayType]
  if (displayType === 'lean_angle' || displayType === 'heading_compass') {
    const factor = size / data.diameter
    return {
      x: widget.data.x,
      y: widget.data.y,
      font_size: scaleNumber(data.font_size, factor, displayType === 'lean_angle' ? 8 : 6, 400),
      display_variants: {
        ...widget.data.display_variants,
        [displayType]: { ...variant, ...(displayType === 'lean_angle' ? buildLeanSize(data, factor) : buildCompassSize(data, factor)) },
      },
    }
  }
  const width = Math.round(size)
  const height = displayType === 'g_force' ? width : Math.round(data.height * (size / data.width))
  const factor = (width / data.width + height / data.height) * 0.5
  let content
  if (displayType === 'g_force') content = buildGForceSize(data, factor)
  else if (['arc', 'corner', 'gt_gauge'].includes(displayType)) content = buildGaugeSize(data, factor, displayType)
  else throw new Error(`Unsupported inspector size policy: ${displayType}`)
  return {
    width,
    height,
    ...(displayType === 'g_force' ? {} : { font_size: scaleNumber(data.font_size, factor, 8, 400) }),
    display_variants: { ...widget.data.display_variants, [displayType]: { ...variant, ...content, width, height } },
  }
}

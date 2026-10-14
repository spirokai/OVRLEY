/** Track dimensions and limits for gauge inspector controls. Rendering belongs to Rust. */

const MIN_BAR_PX = 2

const SUGGESTED_BAR_GAP_PX = 4
const SUGGESTED_LINEAR_BAR_COUNT = 20
const SUGGESTED_ARC_BAR_COUNT_FULL = 60

function validateBarCount(span, bar_count) {
  if (span < MIN_BAR_PX) throw new Error(`Track span must be at least ${MIN_BAR_PX}px`)
  if (!Number.isInteger(bar_count) || bar_count < 1) throw new Error('bar_count must be an integer >= 1')

  const maxCount = Math.floor(span / MIN_BAR_PX)
  if (bar_count > maxCount) throw new Error(`${bar_count} bars do not fit in ${span}px`)
}

function getExactBarGapMax(span, bar_count) {
  return bar_count === 1 ? 0 : (span - bar_count * MIN_BAR_PX) / (bar_count - 1)
}

function getBarGapMax({ span, bar_count }) {
  validateBarCount(span, bar_count)
  return Math.floor(getExactBarGapMax(span, bar_count))
}

export function getBarGeometry({ span, bar_count, bar_gap }) {
  validateBarCount(span, bar_count)
  if (bar_gap < 0) throw new Error('bar_gap must be >= 0')

  if (bar_count === 1) return { count: bar_count, gap: 0, extent: span }

  const maxGap = getExactBarGapMax(span, bar_count)
  const gap = Math.min(bar_gap, maxGap)
  return {
    count: bar_count,
    gap,
    extent: (span - (bar_count - 1) * gap) / bar_count,
  }
}

function getCornerRadiusMax(crossExtent, alongExtent) {
  return Math.floor(Math.min(crossExtent, alongExtent) * 0.5)
}

function getSuggestedBarGeometry({ span, crossExtent, targetBars }) {
  const bar_gap = SUGGESTED_BAR_GAP_PX
  const maxCount = Math.floor(span / MIN_BAR_PX)
  const bar_count = Math.min(maxCount, Math.max(maxCount >= 3 ? 3 : 1, targetBars))
  const maxGap = getBarGapMax({ span, bar_count })
  const gap = Math.min(bar_gap, maxGap)
  const geometry = getBarGeometry({ span, bar_count, bar_gap: gap })
  return { count: bar_count, gap, maxGap, cornerRadiusMax: getCornerRadiusMax(crossExtent, geometry.extent) }
}

export function getLinearBarGeometry({ width, height, orientation, bar_count, bar_gap }) {
  const horizontal = orientation === 'horizontal'
  const span = horizontal ? width : height
  return {
    ...getBarGeometry({
      span,
      bar_count,
      bar_gap,
    }),
    span,
  }
}

export function getSuggestedLinearBarGeometry({ width, height, orientation }) {
  const horizontal = orientation === 'horizontal'
  return getSuggestedBarGeometry({
    span: horizontal ? width : height,
    crossExtent: horizontal ? height : width,
    targetBars: SUGGESTED_LINEAR_BAR_COUNT,
  })
}

export function getLinearTrackCornerRadiusMax(data) {
  const horizontal = data.orientation === 'horizontal'
  const crossExtent = horizontal ? data.height : data.width
  const alongExtent = data.track_fill_style === 'bars' ? getLinearBarGeometry(data).extent : horizontal ? data.width : data.height
  return getCornerRadiusMax(crossExtent, alongExtent)
}

export function getLinearBarGapMax({ width, height, orientation, bar_count }) {
  return getBarGapMax({ span: orientation === 'horizontal' ? width : height, bar_count })
}

export function getSuggestedArcBarGeometry({ radius, sweepAngle, trackThickness, corner = false }) {
  const degrees = Math.abs(sweepAngle)
  const fullCount = corner ? SUGGESTED_ARC_BAR_COUNT_FULL * 1.5 : SUGGESTED_ARC_BAR_COUNT_FULL
  return getSuggestedBarGeometry({
    span: Math.abs((sweepAngle * Math.PI * radius) / 180),
    crossExtent: trackThickness,
    targetBars: Math.round(degrees * (fullCount / 360)),
  })
}

export function getArcTrackCornerRadiusMax(data) {
  const alongExtent = data.track_fill_style === 'bars' ? getArcBarGeometry(data).extent : data.trackThickness
  return getCornerRadiusMax(data.trackThickness, alongExtent)
}

export function getArcBarGapMax({ radius, sweepAngle, bar_count }) {
  return getBarGapMax({ span: Math.abs((sweepAngle * Math.PI * radius) / 180), bar_count })
}

export function getArcBarGeometry({ radius, sweepAngle, bar_count, bar_gap }) {
  const span = Math.abs((sweepAngle * Math.PI * radius) / 180)
  return {
    ...getBarGeometry({
      span,
      bar_count,
      bar_gap,
    }),
    span,
  }
}

/** @param {object} data Active gauge variant. @param {string} displayType Canonical gauge display type. @returns {object} Dimensions used to bound inspector inputs. */
export function getArcTrackDimensions(data, displayType) {
  const frameSize = Math.min(data.width, data.height)
  const corner = displayType === 'corner'
  const capPadding = corner ? Math.min(frameSize, Math.min(data.track_thickness * 0.5, data.track_corner_radius) + data.track_border_thickness) : 0
  return {
    radius: Math.max(0, (corner ? frameSize - capPadding : frameSize * 0.5) - data.track_thickness * 0.5 - data.track_border_thickness),
    sweepAngle: corner ? 90 : data.arc_angle,
    trackThickness: data.track_thickness,
  }
}

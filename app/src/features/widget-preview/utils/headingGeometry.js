/**
 * Heading tape geometry — tick positions, label placement, scroll offsets,
 * and indicator vertex calculations for the heading compass tape widget.
 *
 * Pure functions that mirror the Rust geometry.rs implementation.
 */

const CARDINAL_LABELS = [
  [0, 'N'],
  [45, 'NE'],
  [90, 'E'],
  [135, 'SE'],
  [180, 'S'],
  [225, 'SW'],
  [270, 'W'],
  [315, 'NW'],
]

/**
 * Computes the scroll offset in pixels for a given heading.
 * @param {number} heading - Current heading in degrees (0–360)
 * @param {number} pixelsPerDegree - Horizontal scale
 * @returns {number} Offset in pixels
 */
export function headingOffset(heading, pixelsPerDegree) {
  return heading * pixelsPerDegree
}

/**
 * Checks if a degree value is at a cardinal/intercardinal position (45° multiple).
 * @param {number} degree
 * @returns {boolean}
 */
export function isCardinalDegree(degree) {
  return CARDINAL_LABELS.some(([cardinal]) => Math.abs(degree - cardinal) < 0.01)
}

/**
 * Returns the cardinal label for a degree, or null if not cardinal.
 * @param {number} degree
 * @returns {string|null}
 */
export function cardinalLabelForDegree(degree) {
  const match = CARDINAL_LABELS.find(([cardinal]) => Math.abs(degree - cardinal) < 0.01)
  return match ? match[1] : null
}

/**
 * Computes which ticks are visible within the widget bounds.
 *
 * The tape image is 360 × pixelsPerDegree wide and repeats. We find all
 * tick degree positions whose pixel x falls within [0, width) after
 * accounting for the scroll offset.
 *
 * @param {number} heading - Current heading in degrees (0–360)
 * @param {number} pixelsPerDegree - Horizontal scale
 * @param {number} width - Widget width in pixels
 * @param {number} majorTickInterval - Degrees between major ticks (default 15)
 * @param {number} minorTicksPerMajor - Subdivisions between majors (default 3)
 * @param {boolean} showMajorTicks - Whether to include major ticks
 * @param {boolean} showMinorTicks - Whether to include minor ticks
 * @returns {Array<{degree: number, x: number, isCardinal: boolean, isMajor: boolean}>}
 */
export function visibleTicks(
  heading,
  pixelsPerDegree,
  width,
  majorTickInterval = 15,
  minorTicksPerMajor = 3,
  showMajorTicks = true,
  showMinorTicks = true,
) {
  if (pixelsPerDegree <= 0 || width <= 0) return []

  const tapeWidth = 360 * pixelsPerDegree
  const offset = headingOffset(heading, pixelsPerDegree)
  const minorInterval = majorTickInterval / minorTicksPerMajor

  const ticks = []
  let degree = 0

  while (degree < 360) {
    const isMajor = Math.abs(degree % majorTickInterval) < 0.01
    const isMinor = !isMajor

    if ((isMajor && showMajorTicks) || (isMinor && showMinorTicks)) {
      const tapeX = degree * pixelsPerDegree
      const wrappedX = (((tapeX - offset) % tapeWidth) + tapeWidth) % tapeWidth

      if (wrappedX < width) {
        ticks.push({
          degree,
          x: wrappedX,
          isCardinal: isCardinalDegree(degree),
          isMajor,
        })
      }
    }

    degree += minorInterval
    if (minorInterval <= 0) break
  }

  return ticks
}

/**
 * Computes labels for visible tick positions, with cardinal priority override.
 *
 * Cardinal labels (N/NE/E/SE/S/SW/W/NW) at 45° multiples take priority over
 * numeric labels at the same position.
 *
 * @param {Array<{degree: number, x: number, isCardinal: boolean, isMajor: boolean}>} ticks
 * @param {boolean} showNumericLabels
 * @param {boolean} showCardinalLabels
 * @returns {Array<{degree: number, x: number, text: string, isCardinal: boolean}>}
 */
export function visibleLabels(ticks, showNumericLabels, showCardinalLabels) {
  if (!showNumericLabels && !showCardinalLabels) return []

  const labels = []

  for (const tick of ticks) {
    if (tick.isCardinal && showCardinalLabels) {
      const text = cardinalLabelForDegree(tick.degree)
      if (text) {
        labels.push({ degree: tick.degree, x: tick.x, text, isCardinal: true })
      }
    } else if (showNumericLabels) {
      labels.push({
        degree: tick.degree,
        x: tick.x,
        text: String(Math.round(tick.degree)),
        isCardinal: false,
      })
    }
  }

  return labels
}

/**
 * Computes chevron triangle vertices for a given placement edge.
 *
 * The chevron is an isosceles triangle pointing toward the tape center.
 * At the top edge it points downward; at the bottom edge it points upward.
 *
 * @param {number} centerX - Horizontal center of the widget
 * @param {number} edgeY - Y coordinate of the edge (top or bottom)
 * @param {number} size - Chevron height in pixels
 * @param {boolean} pointingDown - true for top placement
 * @returns {Array<{x: number, y: number}>} 3 vertices
 */
export function chevronVertices(centerX, edgeY, size, pointingDown) {
  const halfBase = size * 0.6
  if (pointingDown) {
    return [
      { x: centerX - halfBase, y: edgeY },
      { x: centerX + halfBase, y: edgeY },
      { x: centerX, y: edgeY + size },
    ]
  }
  return [
    { x: centerX - halfBase, y: edgeY },
    { x: centerX + halfBase, y: edgeY },
    { x: centerX, y: edgeY - size },
  ]
}

/**
 * Computes highlight bar edge marker triangle vertices.
 *
 * Small triangular markers at the top and/or bottom edges of the
 * highlight bar, pointing inward toward the bar center.
 *
 * @param {number} centerX - Horizontal center of the widget
 * @param {number} edgeY - Y coordinate of the edge
 * @param {number} barHalfWidth - Half width of the highlight bar
 * @param {boolean} pointingDown - true for top marker
 * @returns {Array<{x: number, y: number}>} 3 vertices
 */
export function highlightBarMarkerVertices(centerX, edgeY, barHalfWidth, pointingDown) {
  const markerSize = barHalfWidth * 0.4
  if (pointingDown) {
    return [
      { x: centerX - barHalfWidth, y: edgeY },
      { x: centerX + barHalfWidth, y: edgeY },
      { x: centerX, y: edgeY + markerSize },
    ]
  }
  return [
    { x: centerX - barHalfWidth, y: edgeY },
    { x: centerX + barHalfWidth, y: edgeY },
    { x: centerX, y: edgeY - markerSize },
  ]
}

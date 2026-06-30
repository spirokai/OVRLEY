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

export const HEADING_TAPE_LABEL_DESCENT_PCT = 0.25
export const HEADING_TAPE_HIGHLIGHT_BAR_BODY_MARGIN_FRAME_PCT = 0.1

export function headingTapeHasChevron(config, placement) {
  return Boolean(
    config?.show_indicator &&
    config?.indicator_style === 'chevron' &&
    (config?.indicator_placement === placement || config?.indicator_placement === 'both'),
  )
}

export function headingTapeHasHighlightBar(config) {
  return Boolean(config?.show_indicator && config?.indicator_style === 'highlight_bar')
}

export function headingTapeLayout(config) {
  const frameHeight = Math.max(Number(config?.height) || 0, 1)
  const indicatorSize = Math.max(Number(config?.indicator_size) || 0, 0)
  const gap = indicatorSize * 0.5
  const hasTopChevron = headingTapeHasChevron(config, 'top')
  const hasBottomChevron = headingTapeHasChevron(config, 'bottom')
  const highlightBarMargin = headingTapeHasHighlightBar(config) ? frameHeight * HEADING_TAPE_HIGHLIGHT_BAR_BODY_MARGIN_FRAME_PCT : 0
  const idealTopSlot = hasTopChevron ? indicatorSize + gap : highlightBarMargin
  const idealBottomSlot = hasBottomChevron ? indicatorSize + gap : highlightBarMargin
  const availableSlotHeight = Math.max(frameHeight - 1, 0)
  const idealSlotHeight = idealTopSlot + idealBottomSlot
  const slotScale = idealSlotHeight > availableSlotHeight && idealSlotHeight > 0 ? availableSlotHeight / idealSlotHeight : 1
  const topSlot = idealTopSlot * slotScale
  const bottomSlot = idealBottomSlot * slotScale
  const bodyHeight = Math.max(frameHeight - topSlot - bottomSlot, 1)
  const tickScaleHeight = headingTickScaleHeightForRenderedBody(bodyHeight, config)

  return {
    bodyHeight,
    bodyY: topSlot,
    hasBottomChevron,
    hasTopChevron,
    tickScaleHeight,
    totalHeight: frameHeight,
  }
}

export function headingTickPosition(bodyHeight, config, isMajor) {
  const majorLength = (bodyHeight * (Number(config?.major_tick_length_pct) || 0)) / 100
  const minorLength = (bodyHeight * (Number(config?.minor_tick_length_pct) || 0)) / 100
  const length = isMajor ? majorLength : minorLength
  const top = !isMajor && config?.tick_alignment === 'centered' ? (majorLength - minorLength) / 2 : 0

  return { length, top }
}

export function headingLabelBaseline(bodyHeight, config) {
  const majorLength = (bodyHeight * (Number(config?.major_tick_length_pct) || 0)) / 100
  return majorLength + (Number(config?.label_offset) || 0) + (Number(config?.label_font_size) || 0)
}

export function headingLabelBottom(bodyHeight, config) {
  return headingLabelBaseline(bodyHeight, config) + (Number(config?.label_font_size) || 0) * HEADING_TAPE_LABEL_DESCENT_PCT
}

export function headingTapeBodyHeight(bodyHeight, config) {
  return headingLabelBottom(bodyHeight, config)
}

export function headingTickScaleHeightForRenderedBody(renderedBodyHeight, config) {
  const majorTickRatio = Math.max(Number(config?.major_tick_length_pct) || 0, 0.001) / 100
  const fixedLabelStack = (Number(config?.label_offset) || 0) + (Number(config?.label_font_size) || 0) * (1 + HEADING_TAPE_LABEL_DESCENT_PCT)

  return Math.max((renderedBodyHeight - fixedLabelStack) / majorTickRatio, 1)
}

/**
 * Computes the scroll offset in pixels for a given heading.
 * @param {number} heading - Current heading in degrees (0–360)
 * @param {number} pixelsPerDegree - Horizontal scale
 * @param {number} width - Widget width in pixels
 * @returns {number} Offset in pixels
 */
export function headingOffset(heading, pixelsPerDegree, width) {
  return heading * pixelsPerDegree - width / 2
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
  const offset = heading * pixelsPerDegree
  const minorInterval = majorTickInterval / minorTicksPerMajor
  const degrees = new Set(CARDINAL_LABELS.map(([degree]) => degree))

  if (majorTickInterval > 0 && minorTicksPerMajor > 0 && minorInterval > 0) {
    let degree = 0
    while (degree < 360) {
      degrees.add(Math.round(degree * 1000) / 1000)
      degree += minorInterval
    }
  }

  return Array.from(degrees)
    .sort((a, b) => a - b)
    .flatMap((degree) => {
      const isCardinal = isCardinalDegree(degree)
      const isMajor = isCardinal || Math.abs(degree % majorTickInterval) < 0.01
      const isMinor = !isMajor

      if (!isCardinal && !((isMajor && showMajorTicks) || (isMinor && showMinorTicks))) {
        return []
      }

      const tapeX = degree * pixelsPerDegree
      const wrappedX = (((tapeX - offset) % tapeWidth) + tapeWidth) % tapeWidth

      if (wrappedX < width) {
        return [
          {
            degree,
            x: wrappedX,
            isCardinal,
            isMajor,
          },
        ]
      }
      return []
    })
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
export function visibleLabels(ticks, showMinorLabels, showMajorLabels) {
  const labels = []

  for (const tick of ticks) {
    if (tick.isCardinal) {
      const text = cardinalLabelForDegree(tick.degree)
      if (text) {
        labels.push({ degree: tick.degree, x: tick.x, text, isMajorLabel: true })
      }
    } else if (tick.isMajor && showMajorLabels) {
      labels.push({
        degree: tick.degree,
        x: tick.x,
        text: String(Math.round(tick.degree)),
        isMajorLabel: false,
      })
    } else if (!tick.isMajor && showMinorLabels) {
      labels.push({
        degree: tick.degree,
        x: tick.x,
        text: String(Math.round(tick.degree)),
        isMajorLabel: false,
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

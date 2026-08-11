import { getPreviewFontFamily, getPreviewTextBaseline, getPreviewVerticalMetrics, measurePreviewText } from '../../shared/textMeasurement'
import { getPreviewActivity } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { buildLapLogCompletedRows, getLapLogFrameState, getLapTimerDisplayState, LAP_LOG_HEADERS } from './lapTimer'

const LINE_HEIGHT_RATIO = 0.92
const LOG_COLUMN_GAP_RATIO = 1.8
const LOG_ROW_GAP_RATIO = 0.38
const LOG_HEADER_OPACITY = 0.7
const TABLE_VERTICAL_METRICS_TEXT = '0123456789+-:.'

function decimalDigitCount(value) {
  return Math.max(1, Math.floor(Math.log10(Math.max(1, value))) + 1)
}

function getLapLogSizing(activity) {
  if (activity === null) return { lapDigits: 1, timeDigits: 6, timeColons: 1, deltaIntegerDigits: 1 }

  const maxCompletedLapTime = activity.lap_durations_seconds.reduce((maximum, duration) => Math.max(maximum, duration), 0)
  const lastLapStart = activity.lap_start_elapsed_seconds.at(-1)
  const maxCurrentLapTime = lastLapStart === undefined ? 0 : Math.max(0, activity.trim_end_seconds - lastLapStart)
  const maxLapHundredths = Math.round(Math.max(maxCompletedLapTime, maxCurrentLapTime) * 100)
  const usesHours = maxLapHundredths >= 360_000
  const hourDigits = usesHours ? Math.max(2, decimalDigitCount(Math.floor(maxLapHundredths / 360_000))) : 0

  return {
    lapDigits: decimalDigitCount(activity.lap_durations_seconds.length + 1),
    timeDigits: usesHours ? hourDigits + 6 : 6,
    timeColons: usesHours ? 2 : 1,
    deltaIntegerDigits: decimalDigitCount(Math.floor(maxLapHundredths / 100)),
  }
}

function getLapLogColumnRights(activity, fontSize, headerFontSize, fontFamily, labelFontFamily) {
  const sizing = getLapLogSizing(activity)
  const measureRowCharacter = (character) => measurePreviewText(character, fontSize, fontFamily).width
  const widestDigit = Math.max(...'0123456789'.split('').map(measureRowCharacter))
  const widestSign = Math.max(measureRowCharacter('+'), measureRowCharacter('-'))
  const colonWidth = measureRowCharacter(':')
  const decimalPointWidth = measureRowCharacter('.')
  const rowWidths = [
    sizing.lapDigits * widestDigit,
    sizing.timeDigits * widestDigit + sizing.timeColons * colonWidth + decimalPointWidth,
    widestSign + (sizing.deltaIntegerDigits + 2) * widestDigit + decimalPointWidth,
  ]
  const gap = fontSize * LOG_COLUMN_GAP_RATIO
  const columnWidth = (columnIndex) =>
    Math.max(rowWidths[columnIndex], measurePreviewText(LAP_LOG_HEADERS[columnIndex], headerFontSize, labelFontFamily).width)
  const lapRight = columnWidth(0)
  const timeRight = lapRight + gap + columnWidth(1)
  return [lapRight, timeRight, timeRight + gap + columnWidth(2)]
}

function buildLapLogRow({ texts, opacityMultiplier, fontSize, lineHeight, top, deltaColor, color, columnRights, fontFamily }) {
  const verticalMetrics = getPreviewVerticalMetrics(TABLE_VERTICAL_METRICS_TEXT, fontSize, fontFamily)
  const baseline = getPreviewTextBaseline({
    top,
    lineHeight,
    ascent: verticalMetrics.ascent,
    glyphHeight: verticalMetrics.glyphHeight,
  })
  const cells = texts.map((text, columnIndex) => {
    const measure = measurePreviewText(text, fontSize, fontFamily)
    const cellColor = columnIndex === 2 && deltaColor !== null ? deltaColor : color
    return { text, left: columnRights[columnIndex] - measure.width, baseline, measure, color: cellColor }
  })
  return {
    cells,
    opacityMultiplier,
    fontSize,
    fontFamily,
    offsetY: 0,
    valueText: texts.join(' '),
    bounds: {
      minX: Math.min(...cells.map(({ left, measure }) => left - measure.boundsLeft)),
      minY: Math.min(...cells.map((cell) => cell.baseline - cell.measure.ascent)),
      maxX: Math.max(...cells.map(({ left, measure }) => left + measure.boundsRight)),
      maxY: Math.max(...cells.map((cell) => cell.baseline + cell.measure.descent)),
    },
  }
}

/**
 * Prepares invariant lap-log column and header layout.
 * @param {object} params
 * @param {object} params.widget - Lap-log widget configuration.
 * @param {object|null} params.activity - Parsed activity with lap timing data.
 * @returns {object} Prepared lap-log layout.
 */
export function prepareLapLogPreview({ widget, activity }) {
  const fontSize = widget.data.font_size
  const fontFamily = getPreviewFontFamily(widget.data.font)
  const labelFontFamily = getPreviewFontFamily(widget.data.label_font)
  const rowLineHeight = fontSize * LINE_HEIGHT_RATIO
  const rowGap = fontSize * LOG_ROW_GAP_RATIO
  const rowStride = rowLineHeight + rowGap
  const headerFontSize = widget.data.label_font_size
  const headerLineHeight = headerFontSize * LINE_HEIGHT_RATIO
  const dataTop = headerLineHeight + rowGap
  const columnRights = getLapLogColumnRights(activity, fontSize, headerFontSize, fontFamily, labelFontFamily)
  const headerRow = buildLapLogRow({
    texts: LAP_LOG_HEADERS,
    opacityMultiplier: LOG_HEADER_OPACITY,
    fontSize: headerFontSize,
    lineHeight: headerLineHeight,
    top: 0,
    deltaColor: null,
    color: widget.data.label_color,
    columnRights,
    fontFamily: labelFontFamily,
  })
  return { columnRights, dataTop, fontFamily, fontSize, headerRow, rowLineHeight, rowStride }
}

function buildLapLogPreviewModel({ widget, displayActivity, previewSecond, prepared }) {
  const state = getLapLogFrameState(displayActivity, previewSecond)
  const completedRowsTop = prepared.dataTop + (state.currentRow === null ? 0 : prepared.rowStride)
  const completedRows = buildLapLogCompletedRows(displayActivity, state.completedLapCount).map((row, rowIndex) =>
    buildLapLogRow({
      texts: [row.lapText, row.timeText, row.deltaText],
      opacityMultiplier: 1,
      fontSize: prepared.fontSize,
      lineHeight: prepared.rowLineHeight,
      top: completedRowsTop + rowIndex * prepared.rowStride,
      deltaColor: row.useNegativeDeltaColor ? widget.data.negative_delta_color : widget.data.positive_delta_color,
      color: widget.data.color,
      columnRights: prepared.columnRights,
      fontFamily: prepared.fontFamily,
    }),
  )
  const rows = [
    prepared.headerRow,
    ...(state.currentRow
      ? [
          buildLapLogRow({
            texts: [state.currentRow.lapText, state.currentRow.timeText, state.currentRow.deltaText],
            opacityMultiplier: 1,
            fontSize: prepared.fontSize,
            lineHeight: prepared.rowLineHeight,
            top: prepared.dataTop,
            deltaColor: state.currentRow.useNegativeDeltaColor ? widget.data.negative_delta_color : widget.data.positive_delta_color,
            color: widget.data.color,
            columnRights: prepared.columnRights,
            fontFamily: prepared.fontFamily,
          }),
        ]
      : []),
    ...completedRows,
  ]
  const minX = Math.min(...rows.map((row) => row.bounds.minX))
  const minY = Math.min(...rows.map((row) => row.bounds.minY + row.offsetY))
  const maxX = Math.max(...rows.map((row) => row.bounds.maxX))
  const maxY = Math.max(...rows.map((row) => row.bounds.maxY + row.offsetY))

  return {
    content: { type: 'lap_log', rows },
    valueText: rows.map((row) => row.valueText).join('\n'),
    visualBounds: {
      minX,
      minY,
      maxX,
      maxY,
      width: Math.max(maxX - minX, 0),
      height: Math.max(maxY - minY, 0),
      offsetX: -minX,
      offsetY: -minY,
    },
  }
}

/**
 * Builds the intrinsic text layout for a lap timer widget.
 * @param {object} params
 * @param {object} params.widget - Lap timer widget configuration.
 * @param {object|null} params.activity - Parsed activity with lap timing data.
 * @param {number} params.previewSecond - Absolute activity timestamp.
 * @param {object} [params.lapLogPreparation] - Prepared layout, required for lap-log mode.
 * @returns {object} Lap timer preview model.
 */
export function buildLapTimerPreviewModel({ widget, activity, previewSecond, lapLogPreparation }) {
  const fontFamily = getPreviewFontFamily(widget.data.font)
  const labelFontFamily = getPreviewFontFamily(widget.data.label_font)
  const displayActivity = getPreviewActivity(activity, previewSecond)
  if (widget.data.lap_timer_mode === 'lap_log') {
    if (lapLogPreparation === undefined) throw new Error('Lap log preview requires prepared layout')
    return buildLapLogPreviewModel({ widget, displayActivity, previewSecond, prepared: lapLogPreparation })
  }
  const displayState = getLapTimerDisplayState(displayActivity, previewSecond, widget.data.lap_timer_mode)
  const valueText = displayState.valueText
  const valueLineHeight = widget.data.font_size * LINE_HEIGHT_RATIO
  const valueMeasure = measurePreviewText(valueText, widget.data.font_size, fontFamily)
  const valueVerticalMetrics = getPreviewVerticalMetrics(valueText, widget.data.font_size, fontFamily)
  const showLabel = widget.data.show_label
  const labelFontSize = widget.data.label_font_size
  const labelLineHeight = labelFontSize * LINE_HEIGHT_RATIO
  const labelMeasure = showLabel ? measurePreviewText(widget.data.label, labelFontSize, labelFontFamily) : null
  const labelVerticalMetrics = showLabel ? getPreviewVerticalMetrics(widget.data.label, labelFontSize, labelFontFamily) : null
  const labelBaseline = showLabel
    ? getPreviewTextBaseline({
        lineHeight: labelLineHeight,
        ascent: labelVerticalMetrics.ascent,
        glyphHeight: labelVerticalMetrics.glyphHeight,
      })
    : null
  const valueTop = showLabel ? labelLineHeight : 0
  const valueBaseline = getPreviewTextBaseline({
    top: valueTop,
    lineHeight: valueLineHeight,
    ascent: valueVerticalMetrics.ascent,
    glyphHeight: valueVerticalMetrics.glyphHeight,
  })

  const segments = [
    ...(showLabel ? [{ left: 0, baseline: labelBaseline, measure: labelMeasure }] : []),
    { left: 0, baseline: valueBaseline, measure: valueMeasure },
  ]
  const minX = Math.min(...segments.map(({ left, measure }) => left - measure.boundsLeft))
  const minY = Math.min(...segments.map(({ baseline, measure }) => baseline - measure.ascent))
  const maxX = Math.max(...segments.map(({ left, measure }) => left + measure.boundsRight))
  const maxY = Math.max(...segments.map(({ baseline, measure }) => baseline + measure.descent))

  return {
    content: {
      type: 'lap_timer',
      labelText: showLabel ? widget.data.label.toUpperCase() : '',
      valueText,
      valueColor:
        widget.data.lap_timer_mode === 'delta'
          ? displayState.useNegativeDeltaColor
            ? widget.data.negative_delta_color
            : widget.data.positive_delta_color
          : widget.data.color,
      labelFontSize,
      labelBaseline,
      valueBaseline,
    },
    valueText,
    visualBounds: {
      minX,
      minY,
      maxX,
      maxY,
      width: Math.max(maxX - minX, 0),
      height: Math.max(maxY - minY, 0),
      offsetX: -minX,
      offsetY: -minY,
    },
  }
}

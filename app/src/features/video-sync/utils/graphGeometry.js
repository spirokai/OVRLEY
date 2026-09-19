/**
 * Pure timeline graph scale, sampling, path, and event-band geometry.
 */

import {
  VIDEO_SYNC_GRAPH_HEIGHT_PX,
  VIDEO_SYNC_GRAPH_MIN_SCALE_VALUE,
  VIDEO_SYNC_GRAPH_ROBUST_PERCENTILE,
  VIDEO_SYNC_LOCATION_TIMING_TOLERANCE_SECONDS,
  VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS,
} from '../data/videoSyncConstants'
import { secondsToViewPx } from '@/features/player/utils/timelineGeometry'

function getFiniteValues(series) {
  return series.filter((point) => Number.isFinite(point.value)).map((point) => point.value)
}

function getPercentile(values, percentile) {
  if (values.length === 0) return VIDEO_SYNC_GRAPH_MIN_SCALE_VALUE

  const ordered = [...values].sort((left, right) => left - right)
  const position = (ordered.length - 1) * percentile
  const lowerIndex = Math.floor(position)
  const upperIndex = Math.ceil(position)
  const fraction = position - lowerIndex
  return ordered[lowerIndex] + (ordered[upperIndex] - ordered[lowerIndex]) * fraction
}

function getPositiveScale(value) {
  return Math.max(VIDEO_SYNC_GRAPH_MIN_SCALE_VALUE, value)
}

/**
 * Calculates activity-wide display scales. The percentile only clips graph
 * spikes; the source series used by detection and matching is untouched.
 *
 * @param {{speed: {time: number, value: number|null}[], turning: {time: number, value: number|null}[]}} graphSeries Shared detector graph series.
 * @returns {{speed: {min: number, max: number}, turning: {min: number, max: number}}} Fixed graph scales.
 */
export function buildGraphScales(graphSeries) {
  const speedValues = getFiniteValues(graphSeries.speed)
  const turningValues = getFiniteValues(graphSeries.turning).map((value) => Math.abs(value))
  const speedMax = getPositiveScale(getPercentile(speedValues, VIDEO_SYNC_GRAPH_ROBUST_PERCENTILE))
  const turningMax = getPositiveScale(getPercentile(turningValues, VIDEO_SYNC_GRAPH_ROBUST_PERCENTILE))

  return {
    speed: { min: 0, max: speedMax },
    turning: { min: -turningMax, max: turningMax },
  }
}

function findFirstIndexAtOrAfter(series, target) {
  let low = 0
  let high = series.length

  while (low < high) {
    const middle = Math.floor((low + high) / 2)
    if (series[middle].time < target) low = middle + 1
    else high = middle
  }

  return low
}

function findFirstIndexAfter(series, target) {
  let low = 0
  let high = series.length

  while (low < high) {
    const middle = Math.floor((low + high) / 2)
    if (series[middle].time <= target) low = middle + 1
    else high = middle
  }

  return low
}

/**
 * Selects the timestamp range visible in the current viewport.
 *
 * @param {{series: {time: number, value: number|null}[], viewStart: number, viewEnd: number}} options Selection inputs.
 * @returns {{time: number, value: number|null}[]} Visible source samples.
 */
export function selectVisibleGraphSamples({ series, viewStart, viewEnd }) {
  if (series.length === 0 || viewEnd < viewStart) return []
  const startIndex = findFirstIndexAtOrAfter(series, viewStart)
  const endIndex = findFirstIndexAfter(series, viewEnd)
  return series.slice(startIndex, endIndex)
}

function getBucketExtrema(run, { viewStart, viewEnd, widthPx }) {
  const buckets = new Map()
  const columnCount = Math.max(1, Math.floor(widthPx))

  run.forEach((point, index) => {
    const x = secondsToViewPx({ second: point.time, viewStart, viewEnd, widthPx })
    const bucket = Math.max(0, Math.min(columnCount - 1, Math.floor(x)))
    const bucketPoints = buckets.get(bucket)
    if (bucketPoints) bucketPoints.push({ index, point })
    else buckets.set(bucket, [{ index, point }])
  })

  return [...buckets.entries()]
    .sort(([left], [right]) => left - right)
    .flatMap(([, bucketPoints]) => {
      let minimum = bucketPoints[0]
      let maximum = bucketPoints[0]

      for (const candidate of bucketPoints.slice(1)) {
        if (candidate.point.value < minimum.point.value) minimum = candidate
        if (candidate.point.value > maximum.point.value) maximum = candidate
      }

      if (minimum.index === maximum.index) return [minimum.point]
      return minimum.index < maximum.index ? [minimum.point, maximum.point] : [maximum.point, minimum.point]
    })
}

/**
 * Decimates visible samples by horizontal pixel while retaining ordered local
 * minima and maxima. Null samples remain separators between valid runs.
 *
 * @param {{samples: {time: number, value: number|null}[], viewStart: number, viewEnd: number, widthPx: number}} options Decimation inputs.
 * @returns {{time: number, value: number|null}[]} Decimated samples.
 */
export function decimateGraphSamples({ samples, viewStart, viewEnd, widthPx }) {
  if (samples.length === 0 || widthPx <= 0) return []

  const output = []
  let validRun = []
  const flushRun = () => {
    if (validRun.length > 0) output.push(...getBucketExtrema(validRun, { viewStart, viewEnd, widthPx }))
    validRun = []
  }

  for (const sample of samples) {
    if (sample.value === null) {
      flushRun()
      output.push(sample)
    } else {
      validRun.push(sample)
    }
  }
  flushRun()
  return output
}

function mapValueToY(value, scale, heightPx) {
  const ratio = (value - scale.min) / (scale.max - scale.min)
  const clippedRatio = Math.max(0, Math.min(1, ratio))
  return heightPx - clippedRatio * heightPx
}

/**
 * Builds one SVG path for a timestamped graph series.
 *
 * @param {{series: {time: number, value: number|null}[], scale: {min: number, max: number}, viewStart: number, viewEnd: number, widthPx: number, heightPx?: number}} options Path inputs.
 * @returns {string} SVG path data, with null samples creating new subpaths.
 */
export function buildSvgPath({ series, scale, viewStart, viewEnd, widthPx, heightPx = VIDEO_SYNC_GRAPH_HEIGHT_PX }) {
  if (widthPx <= 0 || viewEnd <= viewStart || series.length === 0) return ''

  const visibleSamples = selectVisibleGraphSamples({ series, viewStart, viewEnd })
  const samples = visibleSamples.length > widthPx ? decimateGraphSamples({ samples: visibleSamples, viewStart, viewEnd, widthPx }) : visibleSamples
  const commands = []
  let startsSubpath = true

  for (const sample of samples) {
    if (sample.value === null) {
      startsSubpath = true
      continue
    }
    const x = secondsToViewPx({ second: sample.time, viewStart, viewEnd, widthPx })
    const y = mapValueToY(sample.value, scale, heightPx)
    commands.push(`${startsSubpath ? 'M' : 'L'} ${x} ${y}`)
    startsSubpath = false
  }

  return commands.join(' ')
}

/**
 * Builds both graph paths from one detector result and fixed scales.
 *
 * @param {{graphSeries: {speed: object[], turning: object[]}, scales?: object, viewStart: number, viewEnd: number, widthPx: number, heightPx?: number}} options Graph geometry inputs.
 * @returns {{paths: {speed: string, turning: string}, scales: object, widthPx: number, heightPx: number}} Render-ready graph geometry.
 */
export function buildGraphGeometry({
  graphSeries,
  scales = buildGraphScales(graphSeries),
  viewStart,
  viewEnd,
  widthPx,
  heightPx = VIDEO_SYNC_GRAPH_HEIGHT_PX,
}) {
  return {
    heightPx,
    paths: {
      speed: buildSvgPath({ series: graphSeries.speed, scale: scales.speed, viewStart, viewEnd, widthPx, heightPx }),
      turning: buildSvgPath({ series: graphSeries.turning, scale: scales.turning, viewStart, viewEnd, widthPx, heightPx }),
    },
    scales,
    widthPx,
  }
}

function createBand({ id, type, labelKey, startSecond, endSecond, viewStart, viewEnd, widthPx }) {
  const visibleStart = Math.max(startSecond, viewStart)
  const visibleEnd = Math.min(endSecond, viewEnd)
  if (widthPx <= 0 || visibleEnd <= visibleStart) return null

  const left = secondsToViewPx({ second: visibleStart, viewStart, viewEnd, widthPx })
  const right = secondsToViewPx({ second: visibleEnd, viewStart, viewEnd, widthPx })
  return {
    endSecond,
    id,
    labelKey,
    startSecond,
    style: { left, width: right - left },
    tone: type,
    type,
  }
}

/**
 * Converts detected events to visible graph bands using the shared timing
 * tolerance and the existing timeline time-to-pixel transform.
 *
 * @param {{detection: {stops: object[], turns: object[], location: object|null}|null, viewStart: number, viewEnd: number, widthPx: number, toleranceSeconds?: number}} options Event-band inputs.
 * @returns {object[]} Render-ready event bands.
 */
export function buildEventBands({ detection, viewStart, viewEnd, widthPx, toleranceSeconds = VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS }) {
  if (detection === null || widthPx <= 0 || viewEnd <= viewStart) return []

  const bands = detection.stops
    .map((event) =>
      createBand({
        endSecond: event.time + toleranceSeconds,
        id: event.id,
        labelKey: 'videoSync.stop',
        startSecond: event.time - toleranceSeconds,
        type: event.type,
        viewEnd,
        viewStart,
        widthPx,
      }),
    )
    .concat(
      detection.location === null
        ? []
        : [
            createBand({
              endSecond: detection.location.time + VIDEO_SYNC_LOCATION_TIMING_TOLERANCE_SECONDS,
              id: detection.location.id,
              labelKey: 'videoSync.location',
              startSecond: detection.location.time - VIDEO_SYNC_LOCATION_TIMING_TOLERANCE_SECONDS,
              type: detection.location.type,
              viewEnd,
              viewStart,
              widthPx,
            }),
          ],
    )
    .concat(
      detection.turns.map((event) =>
        createBand({
          endSecond: event.end + toleranceSeconds,
          id: event.id,
          labelKey: event.type === 'leftTurn' ? 'videoSync.leftTurn' : 'videoSync.rightTurn',
          startSecond: event.start - toleranceSeconds,
          type: event.type,
          viewEnd,
          viewStart,
          widthPx,
        }),
      ),
    )

  return bands.filter(Boolean).sort((left, right) => left.startSecond - right.startSecond || left.id.localeCompare(right.id))
}

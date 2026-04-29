function cloneRawSample(sample) {
  return { ...sample }
}

function timestampMs(value) {
  if (!value) return null
  const date = value instanceof Date ? value : new Date(value)
  const time = date.getTime()
  return Number.isFinite(time) ? time : null
}

function median(values) {
  if (!values.length) return null
  const sorted = [...values].sort((left, right) => left - right)
  const middle = Math.floor(sorted.length / 2)
  if (sorted.length % 2 === 0) {
    return (sorted[middle - 1] + sorted[middle]) / 2
  }
  return sorted[middle]
}

function lowerHalfMedian(values) {
  if (!values.length) return null
  const sorted = [...values].sort((left, right) => left - right)
  const cutoff = Math.max(1, Math.ceil(sorted.length / 2))
  return median(sorted.slice(0, cutoff))
}

function zeroFilledIdleSample(
  sample,
  elapsedSeconds,
  timestampMsValue,
  helpers,
) {
  const { roundValue } = helpers
  const synthetic = cloneRawSample(sample)
  synthetic.elapsedSeconds = roundValue(elapsedSeconds, 3)
  synthetic.timestamp =
    timestampMsValue === null ? null : new Date(timestampMsValue).toISOString()
  synthetic.speed = 0
  synthetic.cadence = 0
  synthetic.power = 0
  synthetic.strokeRate = 0
  synthetic.verticalSpeed = 0
  synthetic.gForce = 0
  synthetic.gradient = 0
  synthetic.pace = null
  synthetic.torque = null
  synthetic.strideLength = null
  synthetic.groundContactTime = null
  synthetic.verticalOscillation = null
  synthetic.syntheticIdle = true
  return synthetic
}

function estimateRecordingIntervalSeconds(rawSamples, helpers) {
  const { isFiniteNumber, safeNumber } = helpers
  const deltas = []
  let previousElapsed = null
  let previousTimestampMs = null

  rawSamples.forEach((sample) => {
    const elapsed = safeNumber(sample.elapsedSeconds)
    if (isFiniteNumber(elapsed) && isFiniteNumber(previousElapsed)) {
      const delta = elapsed - previousElapsed
      if (delta > 0 && delta <= 10) deltas.push(delta)
      previousElapsed = elapsed
    } else if (isFiniteNumber(elapsed)) {
      previousElapsed = elapsed
    }

    const currentTimestampMs = timestampMs(sample.timestamp)
    if (currentTimestampMs !== null && previousTimestampMs !== null) {
      const delta = (currentTimestampMs - previousTimestampMs) / 1000
      if (delta > 0 && delta <= 10) deltas.push(delta)
      previousTimestampMs = currentTimestampMs
    } else if (currentTimestampMs !== null) {
      previousTimestampMs = currentTimestampMs
    }
  })

  return lowerHalfMedian(deltas) ?? 1
}

function elapsedSecondsForSample(sample, fallbackOriginTimestampMs, helpers) {
  const { isFiniteNumber, safeNumber } = helpers
  const explicit = safeNumber(sample.elapsedSeconds)
  if (isFiniteNumber(explicit)) return explicit

  const timeMs = timestampMs(sample.timestamp)
  if (timeMs !== null && fallbackOriginTimestampMs !== null) {
    return (timeMs - fallbackOriginTimestampMs) / 1000
  }

  return null
}

function distanceMetersForPair(previousSample, currentSample, helpers) {
  const { haversineDistanceMeters, isFiniteNumber, safeNumber } = helpers
  const previousDistance = safeNumber(previousSample.distance)
  const currentDistance = safeNumber(currentSample.distance)
  if (isFiniteNumber(previousDistance) && isFiniteNumber(currentDistance)) {
    return Math.max(0, currentDistance - previousDistance)
  }

  return haversineDistanceMeters(
    safeNumber(previousSample.latitude),
    safeNumber(previousSample.longitude),
    safeNumber(currentSample.latitude),
    safeNumber(currentSample.longitude),
  )
}

export function buildDistanceSeries(
  coursePoints,
  directDistanceSeries,
  helpers,
) {
  const { haversineDistanceMeters, isFiniteNumber, roundValue } = helpers
  const distanceSeries = []
  let totalDistanceMeters = 0

  for (let index = 0; index < coursePoints.length; index += 1) {
    const directDistance = directDistanceSeries[index]
    if (isFiniteNumber(directDistance)) {
      totalDistanceMeters = Math.max(totalDistanceMeters, directDistance)
      distanceSeries.push(roundValue(totalDistanceMeters, 3))
      continue
    }

    const previousPoint = index > 0 ? coursePoints[index - 1] : null
    const currentPoint = coursePoints[index]
    if (index > 0) {
      totalDistanceMeters += haversineDistanceMeters(
        previousPoint?.[0],
        previousPoint?.[1],
        currentPoint?.[0],
        currentPoint?.[1],
      )
    }

    distanceSeries.push(roundValue(totalDistanceMeters, 3))
  }

  return distanceSeries
}

export function buildElapsedSeries(rawSamples, timeSeries, helpers) {
  const { isFiniteNumber, roundValue, safeNumber } = helpers
  const explicitElapsed = rawSamples.map((sample) =>
    safeNumber(sample.elapsedSeconds),
  )
  const hasExplicitElapsed = explicitElapsed.some(isFiniteNumber)

  if (hasExplicitElapsed) {
    const elapsedSeries = []
    let lastValue = 0

    for (let index = 0; index < explicitElapsed.length; index += 1) {
      const currentExplicit = explicitElapsed[index]
      if (isFiniteNumber(currentExplicit)) {
        lastValue = Math.max(lastValue, currentExplicit)
        elapsedSeries.push(roundValue(lastValue, 3))
        continue
      }

      if (index === 0) {
        elapsedSeries.push(0)
        continue
      }

      lastValue = elapsedSeries[index - 1]
      elapsedSeries.push(roundValue(lastValue, 3))
    }

    return elapsedSeries
  }

  const validTimestamps = timeSeries.map((value) =>
    value ? new Date(value) : null,
  )
  const origin = validTimestamps.find(
    (value) => value && Number.isFinite(value.getTime()),
  )

  if (!origin) {
    return rawSamples.map((_, index) => roundValue(index, 3))
  }

  let lastValue = 0
  return validTimestamps.map((timestamp, index) => {
    if (!timestamp || !Number.isFinite(timestamp.getTime())) {
      lastValue = index === 0 ? 0 : lastValue
      return roundValue(lastValue, 3)
    }

    const nextValue = Math.max(
      0,
      (timestamp.getTime() - origin.getTime()) / 1000,
    )

    if (nextValue <= lastValue && index > 0) {
      lastValue += 0.001
      return roundValue(lastValue, 3)
    }

    lastValue = nextValue
    return roundValue(nextValue, 3)
  })
}

export function buildProgressSeries(distanceSeries, helpers) {
  const { isFiniteNumber, roundValue } = helpers
  const totalDistanceMeters = distanceSeries[distanceSeries.length - 1] ?? 0
  if (!isFiniteNumber(totalDistanceMeters) || totalDistanceMeters <= 0) {
    return distanceSeries.map(() => 0)
  }

  return distanceSeries.map((value) =>
    roundValue(value / totalDistanceMeters, 6),
  )
}

export function insertIdleGapSamples(rawSamples, helpers) {
  const { isFiniteNumber, roundValue, safeTimestamp } = helpers

  if (rawSamples.length < 2) {
    return {
      rawSamples,
      gapDebug: {
        detected_gaps: [],
        inserted_sample_count: 0,
        recording_interval_seconds: 1,
      },
    }
  }

  const originTimestampMs = rawSamples
    .map((sample) => timestampMs(sample.timestamp))
    .find((value) => value !== null)
  const recordingIntervalSeconds = Math.max(
    0.2,
    estimateRecordingIntervalSeconds(rawSamples, helpers),
  )
  const gapThresholdSeconds = Math.max(3, recordingIntervalSeconds * 3)
  const stationaryDistanceThresholdMeters = Math.max(
    5,
    recordingIntervalSeconds * 2.5,
  )
  const detectedGaps = []
  const filledSamples = [cloneRawSample(rawSamples[0])]
  let insertedSampleCount = 0

  for (let index = 1; index < rawSamples.length; index += 1) {
    const previousSample = rawSamples[index - 1]
    const currentSample = rawSamples[index]
    const previousElapsed = elapsedSecondsForSample(
      previousSample,
      originTimestampMs,
      helpers,
    )
    const currentElapsed = elapsedSecondsForSample(
      currentSample,
      originTimestampMs,
      helpers,
    )
    const elapsedDelta =
      isFiniteNumber(previousElapsed) && isFiniteNumber(currentElapsed)
        ? currentElapsed - previousElapsed
        : null

    let insertedForGap = 0
    if (isFiniteNumber(elapsedDelta) && elapsedDelta > gapThresholdSeconds) {
      const distanceDelta = distanceMetersForPair(
        previousSample,
        currentSample,
        helpers,
      )
      if (distanceDelta <= stationaryDistanceThresholdMeters) {
        const previousTimestampMs = timestampMs(previousSample.timestamp)
        const currentTimestampMs = timestampMs(currentSample.timestamp)
        const maxInsertionCount =
          Math.floor(elapsedDelta / recordingIntervalSeconds) - 1

        for (
          let insertIndex = 1;
          insertIndex <= maxInsertionCount;
          insertIndex += 1
        ) {
          const syntheticElapsed =
            previousElapsed + recordingIntervalSeconds * insertIndex
          if (syntheticElapsed >= currentElapsed - 1e-6) break

          let syntheticTimestampMs = null
          if (
            previousTimestampMs !== null &&
            currentTimestampMs !== null &&
            currentTimestampMs > previousTimestampMs
          ) {
            syntheticTimestampMs = Math.min(
              currentTimestampMs,
              previousTimestampMs +
                Math.round(recordingIntervalSeconds * 1000 * insertIndex),
            )
          }

          filledSamples.push(
            zeroFilledIdleSample(
              previousSample,
              syntheticElapsed,
              syntheticTimestampMs,
              helpers,
            ),
          )
          insertedForGap += 1
        }
      }
    }

    if (insertedForGap > 0) {
      detectedGaps.push({
        start_index: index - 1,
        end_index: index,
        start_elapsed_seconds: roundValue(previousElapsed, 3),
        end_elapsed_seconds: roundValue(currentElapsed, 3),
        gap_seconds: roundValue(elapsedDelta, 3),
        inserted_samples: insertedForGap,
        start_timestamp: safeTimestamp(previousSample.timestamp),
        end_timestamp: safeTimestamp(currentSample.timestamp),
      })
      insertedSampleCount += insertedForGap
    }

    filledSamples.push(cloneRawSample(currentSample))
  }

  return {
    rawSamples: filledSamples,
    gapDebug: {
      detected_gaps: detectedGaps,
      inserted_sample_count: insertedSampleCount,
      recording_interval_seconds: roundValue(recordingIntervalSeconds, 3),
      gap_threshold_seconds: roundValue(gapThresholdSeconds, 3),
      stationary_distance_threshold_m: roundValue(
        stationaryDistanceThresholdMeters,
        3,
      ),
    },
  }
}

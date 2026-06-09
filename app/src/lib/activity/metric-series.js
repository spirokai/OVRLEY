/**
 * Metric series derivation — gradient, heading, pace, torque, elevation smoothing.
 */

/**
 * Handles smooth elevation series.
 *
 * @param {*} elevationSeries - Value for elevation series.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @param {*} radius - Numeric radius value.
 * @returns {*} Result produced by the helper.
 */
function smoothElevationSeries(elevationSeries, helpers, radius = 2) {
  const { isFiniteNumber, roundValue } = helpers

  return elevationSeries.map((value, index) => {
    if (!isFiniteNumber(value)) return null

    let total = 0
    let count = 0
    for (let neighborIndex = Math.max(0, index - radius); neighborIndex <= Math.min(elevationSeries.length - 1, index + radius); neighborIndex += 1) {
      const neighborValue = elevationSeries[neighborIndex]
      if (!isFiniteNumber(neighborValue)) continue
      total += neighborValue
      count += 1
    }

    return count > 0 ? roundValue(total / count, 3) : null
  })
}

/**
 * Applies fixed savitzky golay.
 *
 * @param {*} values - Input values processed by the helper.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @param {*} coefficients - Value for coefficients.
 * @returns {*} Result produced by the helper.
 */
function applyFixedSavitzkyGolay(values, helpers, coefficients) {
  const { isFiniteNumber, roundValue } = helpers
  const radius = Math.floor(coefficients.length / 2)
  return values.map((value, index) => {
    if (!isFiniteNumber(value)) return null

    let total = 0
    let coefficientTotal = 0
    for (let offset = -radius; offset <= radius; offset += 1) {
      const neighborIndex = index + offset
      if (neighborIndex < 0 || neighborIndex >= values.length) continue

      const neighborValue = values[neighborIndex]
      if (!isFiniteNumber(neighborValue)) continue

      const coefficient = coefficients[offset + radius]
      total += neighborValue * coefficient
      coefficientTotal += coefficient
    }

    if (!coefficientTotal) {
      return roundValue(value, 3)
    }

    return roundValue(total / coefficientTotal, 3)
  })
}

/**
 * Handles smooth gradient input series legacy.
 *
 * @param {*} elevationSeries - Value for elevation series.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @returns {*} Result produced by the helper.
 */
function smoothGradientInputSeriesLegacy(elevationSeries, helpers) {
  const { isFiniteNumber, roundValue } = helpers
  const populatedSamples = elevationSeries.filter(isFiniteNumber).length
  if (populatedSamples < 3) {
    return elevationSeries.map((value) => roundValue(value, 3))
  }

  return applyFixedSavitzkyGolay(elevationSeries, helpers, [-2, 3, 6, 7, 6, 3, -2])
}

/**
 * Derives gradient series.
 *
 * @param {*} elevationSeries - Value for elevation series.
 * @param {*} distanceSeries - Value for distance series.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @returns {*} Derived data structure for downstream use.
 */
function deriveGradientSeries(elevationSeries, distanceSeries, helpers) {
  const { isFiniteNumber, roundValue } = helpers
  const smoothedElevation = smoothElevationSeries(elevationSeries, helpers)
  const gradientSeries = []
  let lastGradient = 0

  for (let index = 0; index < distanceSeries.length; index += 1) {
    const currentDistance = distanceSeries[index]
    if (!isFiniteNumber(currentDistance)) {
      gradientSeries.push(null)
      continue
    }

    let leftIndex = index
    while (leftIndex > 0 && currentDistance - distanceSeries[leftIndex] < 5) {
      leftIndex -= 1
    }

    let rightIndex = index
    while (rightIndex < distanceSeries.length - 1 && distanceSeries[rightIndex] - currentDistance < 5) {
      rightIndex += 1
    }

    const leftDistance = distanceSeries[leftIndex]
    const rightDistance = distanceSeries[rightIndex]
    const leftElevation = smoothedElevation[leftIndex]
    const rightElevation = smoothedElevation[rightIndex]

    if (!isFiniteNumber(leftDistance) || !isFiniteNumber(rightDistance) || !isFiniteNumber(leftElevation) || !isFiniteNumber(rightElevation)) {
      gradientSeries.push(roundValue(lastGradient, 3))
      continue
    }

    const horizontalDistance = rightDistance - leftDistance
    if (horizontalDistance < 1) {
      gradientSeries.push(roundValue(lastGradient, 3))
      continue
    }

    const nextGradient = Math.max(-30, Math.min(30, ((rightElevation - leftElevation) / horizontalDistance) * 100))
    lastGradient = nextGradient
    gradientSeries.push(roundValue(nextGradient, 3))
  }

  return gradientSeries
}

/**
 * Derives legacy gradient series.
 *
 * @param {*} elevationSeries - Value for elevation series.
 * @param {*} distanceSeries - Value for distance series.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @returns {*} Derived data structure for downstream use.
 */
function deriveLegacyGradientSeries(elevationSeries, distanceSeries, helpers) {
  const { isFiniteNumber, roundValue } = helpers

  if (!elevationSeries.length) {
    return []
  }

  if (elevationSeries.length === 1 || distanceSeries.length !== elevationSeries.length) {
    return elevationSeries.map(() => 0)
  }

  const gradientInputElevations = smoothGradientInputSeriesLegacy(elevationSeries, helpers)
  const gradients = []
  const lastIndex = elevationSeries.length - 1

  for (let index = 0; index < elevationSeries.length; index += 1) {
    const leftIndex = index === 0 ? 0 : index - 1
    const rightIndex = index === lastIndex ? lastIndex : index + 1

    const leftDistance = distanceSeries[leftIndex]
    const rightDistance = distanceSeries[rightIndex]
    const leftElevation = gradientInputElevations[leftIndex]
    const rightElevation = gradientInputElevations[rightIndex]

    if (!isFiniteNumber(leftDistance) || !isFiniteNumber(rightDistance) || !isFiniteNumber(leftElevation) || !isFiniteNumber(rightElevation)) {
      gradients.push(0)
      continue
    }

    const horizontalDistance = rightDistance - leftDistance
    if (horizontalDistance <= 0) {
      gradients.push(0)
      continue
    }

    gradients.push(roundValue(((rightElevation - leftElevation) / horizontalDistance) * 100, 3) ?? 0)
  }

  return gradients
}

/**
 * Derives heading series using a minimum-distance lookback.
 *
 * Instead of using adjacent course samples, this derives bearing from a
 * minimum-distance baseline around the current sample. When possible it uses
 * one point before and one point after the current sample so a single noisy
 * fix does not dominate the bearing.
 *
 * @param {*} courseSeries - Value for course series.
 * @param {*} distanceSeries - Cumulative distance series aligned with courseSeries.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @param {number} [minDistanceMeters=2] - Minimum travel distance before recomputing bearing.
 * @returns {*} Derived data structure for downstream use.
 */
function deriveHeadingSeries(courseSeries, distanceSeries, helpers, minDistanceMeters = 2) {
  const { calculateBearingDegrees, isFiniteNumber, roundValue } = helpers
  const derivedSeries = []
  let lastHeading = null
  const halfBaselineMeters = minDistanceMeters / 2

  for (let index = 0; index < courseSeries.length; index += 1) {
    const currentPoint = courseSeries[index]
    const currentDistance = distanceSeries[index]
    let heading = null

    if (isFiniteNumber(currentDistance)) {
      let centeredLookbackIndex = index - 1
      while (centeredLookbackIndex >= 0 && currentDistance - distanceSeries[centeredLookbackIndex] < halfBaselineMeters) {
        centeredLookbackIndex -= 1
      }

      let lookaheadIndex = index + 1
      while (lookaheadIndex < courseSeries.length && distanceSeries[lookaheadIndex] - currentDistance < halfBaselineMeters) {
        lookaheadIndex += 1
      }

      let fallbackLookbackIndex = index - 1
      while (fallbackLookbackIndex >= 0 && currentDistance - distanceSeries[fallbackLookbackIndex] < minDistanceMeters) {
        fallbackLookbackIndex -= 1
      }

      const hasCenteredLookback = centeredLookbackIndex >= 0 && isFiniteNumber(distanceSeries[centeredLookbackIndex])
      const hasLookahead = lookaheadIndex < courseSeries.length && isFiniteNumber(distanceSeries[lookaheadIndex])
      const hasFallbackLookback = fallbackLookbackIndex >= 0 && isFiniteNumber(distanceSeries[fallbackLookbackIndex])

      if (hasCenteredLookback && hasLookahead) {
        heading = calculateBearingDegrees(courseSeries[centeredLookbackIndex], courseSeries[lookaheadIndex])
      } else if (hasFallbackLookback) {
        heading = calculateBearingDegrees(courseSeries[fallbackLookbackIndex], currentPoint)
      }
    }

    if (isFiniteNumber(heading)) {
      lastHeading = roundValue(heading, 3)
    }

    derivedSeries.push(lastHeading)
  }

  return derivedSeries
}

/**
 * Smooths heading as a circular signal using exponential smoothing on unit vectors.
 *
 * Smoothing sin/cos components avoids 0°/360° wrap artifacts and preserves
 * continuous turns better than smoothing raw degree values.
 *
 * @param {Array<number|null>} headingSeries - Derived heading values in degrees.
 * @param {*} helpers - Shared numeric helper functions.
 * @param {number} [alpha=0.2] - EMA smoothing factor.
 * @returns {Array<number|null>} Smoothed heading series.
 */
function smoothHeadingSeriesCircularEma(headingSeries, helpers, alpha = 0.05) {
  const { isFiniteNumber, roundValue } = helpers
  const smoothedSeries = []
  let smoothedX = null
  let smoothedY = null

  for (const heading of headingSeries) {
    if (!isFiniteNumber(heading)) {
      smoothedSeries.push(null)
      continue
    }

    const radians = (heading * Math.PI) / 180
    const nextX = Math.cos(radians)
    const nextY = Math.sin(radians)

    if (!isFiniteNumber(smoothedX) || !isFiniteNumber(smoothedY)) {
      smoothedX = nextX
      smoothedY = nextY
    } else {
      smoothedX = alpha * nextX + (1 - alpha) * smoothedX
      smoothedY = alpha * nextY + (1 - alpha) * smoothedY
    }

    const smoothedHeading = (Math.atan2(smoothedY, smoothedX) * 180) / Math.PI
    smoothedSeries.push(roundValue((smoothedHeading + 360) % 360, 3))
  }

  return smoothedSeries
}

/**
 * Derives numeric rate series.
 *
 * @param {*} numeratorSeries - Value for numerator series.
 * @param {*} elapsedSeries - Elapsed time series for the activity.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @returns {*} Derived data structure for downstream use.
 */
function deriveNumericRateSeries(numeratorSeries, elapsedSeries, helpers) {
  const { isFiniteNumber, roundValue } = helpers
  const derivedSeries = []
  let lastValue = null

  for (let index = 0; index < numeratorSeries.length; index += 1) {
    if (index === 0) {
      derivedSeries.push(null)
      continue
    }

    const previousValue = numeratorSeries[index - 1]
    const currentValue = numeratorSeries[index]
    const previousElapsed = elapsedSeries[index - 1]
    const currentElapsed = elapsedSeries[index]

    if (!isFiniteNumber(previousValue) || !isFiniteNumber(currentValue) || !isFiniteNumber(previousElapsed) || !isFiniteNumber(currentElapsed)) {
      derivedSeries.push(lastValue)
      continue
    }

    const elapsedDelta = currentElapsed - previousElapsed
    if (elapsedDelta <= 0) {
      derivedSeries.push(lastValue)
      continue
    }

    lastValue = (currentValue - previousValue) / elapsedDelta
    derivedSeries.push(roundValue(lastValue, 6))
  }

  return derivedSeries
}

/**
 * Derives pace series.
 *
 * @param {*} speedSeries - Value for speed series.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @returns {*} Derived data structure for downstream use.
 */
function derivePaceSeries(speedSeries, helpers) {
  const { isFiniteNumber, roundValue } = helpers
  return speedSeries.map((speed) => {
    if (!isFiniteNumber(speed) || speed <= 0) return null
    return roundValue(1000 / speed, 3)
  })
}

/**
 * Derives torque series.
 *
 * @param {*} powerSeries - Value for power series.
 * @param {*} cadenceSeries - Value for cadence series.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @returns {*} Derived data structure for downstream use.
 */
function deriveTorqueSeries(powerSeries, cadenceSeries, helpers) {
  const { isFiniteNumber, roundValue } = helpers
  return powerSeries.map((power, index) => {
    const cadence = cadenceSeries[index]
    if (!isFiniteNumber(power) || !isFiniteNumber(cadence) || cadence <= 0) {
      return null
    }

    const angularVelocity = (2 * Math.PI * cadence) / 60
    if (!isFiniteNumber(angularVelocity) || angularVelocity <= 0) return null
    return roundValue(power / angularVelocity, 6)
  })
}

/**
 * Combines series.
 *
 * @param {*} directSeries - Value for direct series.
 * @param {*} derivedSeries - Value for derived series.
 * @returns {object} Result produced by the helper.
 */
/**
 * Combines a primary series with a fallback series at each index.
 * @param {number[]} primarySeries - Values to prefer (direct or derived depending on options).
 * @param {number[]} fallbackSeries - Values to use when primary is null.
 * @param {{ preferDerived?: boolean }} [options] - When true, source labels reflect derived-first preference.
 * @returns {{ series: number[], source: 'direct'|'derived'|'mixed'|'missing' }}
 */
function combineSeries(primarySeries, fallbackSeries, { preferDerived = false } = {}) {
  const combinedSeries = primarySeries.map((value, index) => value ?? fallbackSeries[index] ?? null)

  const primaryCount = primarySeries.filter((value) => value !== null).length
  const fallbackOnlyCount = combinedSeries.filter((value, index) => value !== null && primarySeries[index] === null).length

  const primarySource = preferDerived ? 'derived' : 'direct'
  const fallbackSource = preferDerived ? 'direct' : 'derived'

  let source = 'missing'
  if (primaryCount > 0 && fallbackOnlyCount > 0) {
    source = 'mixed'
  } else if (primaryCount > 0) {
    source = primarySource
  } else if (fallbackOnlyCount > 0) {
    source = fallbackSource
  }

  return { series: combinedSeries, source }
}

/**
 * Builds metric coverage.
 *
 * @param {*} metricSeriesMap - Metric series keyed by metric identifier.
 * @returns {*} Derived data structure for downstream use.
 */
export function buildMetricCoverage(metricSeriesMap) {
  return Object.fromEntries(
    Object.entries(metricSeriesMap).map(([metric, descriptor]) => {
      const values = descriptor.series
      const availableCount = values.filter((value) => value !== null).length
      return [
        metric,
        {
          source: descriptor.source,
          availableCount,
          totalSamples: values.length,
        },
      ]
    }),
  )
}

/**
 * Derives numeric rate series using a fixed-duration lookback window.
 *
 * Instead of differencing consecutive samples (which amplifies quantization
 * noise in dense telemetry), this function looks back a configurable number
 * of seconds and computes the rate over that window.  This naturally handles
 * data sources where the underlying sensor updates at a slower rate than the
 * sample cadence (e.g. DJI SRT where GPS fixes arrive ~6-10 Hz but cue data
 * is recorded at ~30 Hz).
 *
 * @param {number[]} numeratorSeries - Value series (distance, elevation, etc.).
 * @param {number[]} elapsedSeries - Elapsed time series aligned with numeratorSeries.
 * @param {object} helpers - Shared numeric helpers ({ isFiniteNumber, roundValue }).
 * @param {number} [windowSec=1] - Lookback window in seconds.
 * @returns {number[]} Derived rate series (null at index 0).
 */
function deriveWindowedRateSeries(numeratorSeries, elapsedSeries, helpers, windowSec = 1) {
  const { isFiniteNumber, roundValue } = helpers
  const derivedSeries = []
  let lastValue = null

  for (let index = 0; index < numeratorSeries.length; index += 1) {
    const currentValue = numeratorSeries[index]
    const currentElapsed = elapsedSeries[index]

    if (!isFiniteNumber(currentValue) || !isFiniteNumber(currentElapsed)) {
      derivedSeries.push(lastValue)
      continue
    }

    // Walk backward from current index to find a sample at least windowSec ago
    const lookbackTarget = currentElapsed - windowSec
    let lookbackIndex = index - 1
    while (lookbackIndex >= 0 && elapsedSeries[lookbackIndex] > lookbackTarget) {
      lookbackIndex -= 1
    }

    if (lookbackIndex < 0) {
      derivedSeries.push(lastValue)
      continue
    }

    const lookbackValue = numeratorSeries[lookbackIndex]
    const lookbackElapsed = elapsedSeries[lookbackIndex]

    if (!isFiniteNumber(lookbackValue) || !isFiniteNumber(lookbackElapsed)) {
      derivedSeries.push(lastValue)
      continue
    }

    const elapsedDelta = currentElapsed - lookbackElapsed
    if (elapsedDelta <= 0) {
      derivedSeries.push(lastValue)
      continue
    }

    lastValue = (currentValue - lookbackValue) / elapsedDelta
    derivedSeries.push(roundValue(lastValue, 6))
  }

  return derivedSeries
}

/**
 * Derives activity metric series.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.courseSeries - Value for course series.
 * @param {*} options.distanceSeries - Value for distance series.
 * @param {*} options.elevationBaseSeries - Value for elevation base series.
 * @param {*} options.elapsedSeries - Elapsed time series for the activity.
 * @param {*} options.normalizedRawSamples - Value for normalized raw samples.
 * @param {*} options.useLegacyGpxDerivations - Value for use legacy gpx derivations.
 * @param {*} options.helpers - Shared numeric and geospatial helper functions.
 * @returns {object} Derived data structure for downstream use.
 */
export function deriveActivityMetricSeries({
  courseSeries,
  distanceSeries,
  elevationBaseSeries,
  elapsedSeries,
  normalizedRawSamples,
  useLegacyGpxDerivations,
  helpers,
  useWindowedRate = false,
  rateWindowSeconds = 1,
}) {
  const { safeNumber } = helpers
  const directMetrics = {
    air_pressure: normalizedRawSamples.map((sample) => safeNumber(sample.airPressure)),
    altitude: normalizedRawSamples.map((sample) => safeNumber(sample.altitude)),
    cadence: normalizedRawSamples.map((sample) => safeNumber(sample.cadence)),
    core_temperature: normalizedRawSamples.map((sample) => safeNumber(sample.coreTemperature)),
    distance: distanceSeries,
    elevation: elevationBaseSeries,
    g_force: normalizedRawSamples.map((sample) => safeNumber(sample.gForce)),
    gear_position: normalizedRawSamples.map((sample) => safeNumber(sample.gearPosition)),
    gradient: normalizedRawSamples.map((sample) => safeNumber(sample.gradient)),
    ground_contact_time: normalizedRawSamples.map((sample) => safeNumber(sample.groundContactTime)),
    heading: normalizedRawSamples.map((sample) => safeNumber(sample.heading)),
    heartrate: normalizedRawSamples.map((sample) => safeNumber(sample.heartrate)),
    left_right_balance: normalizedRawSamples.map((sample) => sample.leftRightBalance ?? null),
    pace: normalizedRawSamples.map((sample) => safeNumber(sample.pace)),
    power: normalizedRawSamples.map((sample) => safeNumber(sample.power)),
    speed: normalizedRawSamples.map((sample) => safeNumber(sample.speed)),
    stride_length: normalizedRawSamples.map((sample) => safeNumber(sample.strideLength)),
    stroke_rate: normalizedRawSamples.map((sample) => safeNumber(sample.strokeRate)),
    temperature: normalizedRawSamples.map((sample) => safeNumber(sample.temperature)),
    torque: normalizedRawSamples.map((sample) => safeNumber(sample.torque)),
    vertical_oscillation: normalizedRawSamples.map((sample) => safeNumber(sample.verticalOscillation)),
    vertical_speed: normalizedRawSamples.map((sample) => safeNumber(sample.verticalSpeed)),
    iso: normalizedRawSamples.map((sample) => safeNumber(sample.iso)),
    aperture: normalizedRawSamples.map((sample) => safeNumber(sample.aperture)),
    shutter_speed: normalizedRawSamples.map((sample) => safeNumber(sample.shutterSpeed)),
    focal_length: normalizedRawSamples.map((sample) => safeNumber(sample.focalLength)),
    ev: normalizedRawSamples.map((sample) => safeNumber(sample.ev)),
    color_temperature: normalizedRawSamples.map((sample) => safeNumber(sample.colorTemperature)),
  }

  const nullSeries = normalizedRawSamples.map(() => null)
  const derivedSpeed = useWindowedRate
    ? deriveWindowedRateSeries(distanceSeries, elapsedSeries, helpers, rateWindowSeconds)
    : deriveNumericRateSeries(distanceSeries, elapsedSeries, helpers)
  const derivedHeading = smoothHeadingSeriesCircularEma(deriveHeadingSeries(courseSeries, distanceSeries, helpers), helpers)
  const derivedGradient = useLegacyGpxDerivations
    ? deriveLegacyGradientSeries(directMetrics.elevation, distanceSeries, helpers)
    : deriveGradientSeries(directMetrics.elevation, distanceSeries, helpers)
  const derivedVerticalSpeed = useWindowedRate
    ? deriveWindowedRateSeries(directMetrics.elevation, elapsedSeries, helpers, rateWindowSeconds)
    : deriveNumericRateSeries(directMetrics.elevation, elapsedSeries, helpers)
  const derivedPace = derivePaceSeries(
    directMetrics.speed.map((value, index) => value ?? derivedSpeed[index]),
    helpers,
  )
  const derivedTorque = deriveTorqueSeries(directMetrics.power, directMetrics.cadence, helpers)

  return {
    directMetrics,
    metricSeriesMap: {
      air_pressure: combineSeries(directMetrics.air_pressure, nullSeries),
      altitude: combineSeries(directMetrics.altitude, directMetrics.elevation),
      cadence: combineSeries(directMetrics.cadence, nullSeries),
      core_temperature: combineSeries(directMetrics.core_temperature, nullSeries),
      distance: { series: directMetrics.distance, source: 'direct' },
      elevation: combineSeries(directMetrics.elevation, nullSeries),
      g_force: combineSeries(directMetrics.g_force, nullSeries),
      gear_position: combineSeries(directMetrics.gear_position, nullSeries),
      gradient: combineSeries(derivedGradient, directMetrics.gradient, { preferDerived: true }),
      ground_contact_time: combineSeries(directMetrics.ground_contact_time, nullSeries),
      heading: combineSeries(directMetrics.heading, derivedHeading),
      heartrate: combineSeries(directMetrics.heartrate, nullSeries),
      left_right_balance: combineSeries(directMetrics.left_right_balance, nullSeries),
      pace: combineSeries(directMetrics.pace, derivedPace),
      power: combineSeries(directMetrics.power, nullSeries),
      speed: combineSeries(directMetrics.speed, derivedSpeed),
      stride_length: combineSeries(directMetrics.stride_length, nullSeries),
      stroke_rate: combineSeries(directMetrics.stroke_rate, nullSeries),
      temperature: combineSeries(directMetrics.temperature, nullSeries),
      torque: combineSeries(directMetrics.torque, derivedTorque),
      vertical_oscillation: combineSeries(directMetrics.vertical_oscillation, nullSeries),
      vertical_speed: combineSeries(directMetrics.vertical_speed, derivedVerticalSpeed),
      iso: combineSeries(directMetrics.iso, nullSeries),
      aperture: combineSeries(directMetrics.aperture, nullSeries),
      shutter_speed: combineSeries(directMetrics.shutter_speed, nullSeries),
      focal_length: combineSeries(directMetrics.focal_length, nullSeries),
      ev: combineSeries(directMetrics.ev, nullSeries),
      color_temperature: combineSeries(directMetrics.color_temperature, nullSeries),
    },
  }
}

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
 * Derives heading series.
 *
 * @param {*} courseSeries - Value for course series.
 * @param {*} helpers - Shared numeric and geospatial helper functions.
 * @returns {*} Derived data structure for downstream use.
 */
function deriveHeadingSeries(courseSeries, helpers) {
  const { calculateBearingDegrees, isFiniteNumber, roundValue } = helpers
  let lastHeading = null

  return courseSeries.map((point, index) => {
    const previousPoint = index > 0 ? courseSeries[index - 1] : null
    const nextPoint = index < courseSeries.length - 1 ? courseSeries[index + 1] : null
    const heading = calculateBearingDegrees(previousPoint, point) ?? calculateBearingDegrees(point, nextPoint) ?? lastHeading

    if (isFiniteNumber(heading)) {
      lastHeading = heading
      return roundValue(heading, 3)
    }

    return null
  })
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
function combineSeries(directSeries, derivedSeries) {
  const combinedSeries = directSeries.map((value, index) => value ?? derivedSeries[index] ?? null)

  const directCount = directSeries.filter((value) => value !== null).length
  const derivedOnlyCount = combinedSeries.filter((value, index) => value !== null && directSeries[index] === null).length

  let source = 'missing'
  if (directCount > 0 && derivedOnlyCount > 0) {
    source = 'mixed'
  } else if (directCount > 0) {
    source = 'direct'
  } else if (derivedOnlyCount > 0) {
    source = 'derived'
  }

  return {
    series: combinedSeries,
    source,
  }
}

/**
 * Combines series prefer derived.
 *
 * @param {*} derivedSeries - Value for derived series.
 * @param {*} directSeries - Value for direct series.
 * @returns {object} Result produced by the helper.
 */
function combineSeriesPreferDerived(derivedSeries, directSeries) {
  const combinedSeries = derivedSeries.map((value, index) => value ?? directSeries[index] ?? null)

  const derivedCount = derivedSeries.filter((value) => value !== null).length
  const directFallbackCount = combinedSeries.filter((value, index) => value !== null && derivedSeries[index] === null).length

  let source = 'missing'
  if (derivedCount > 0 && directFallbackCount > 0) {
    source = 'mixed'
  } else if (derivedCount > 0) {
    source = 'derived'
  } else if (directFallbackCount > 0) {
    source = 'direct'
  }

  return {
    series: combinedSeries,
    source,
  }
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
  }

  const nullSeries = normalizedRawSamples.map(() => null)
  const derivedSpeed = deriveNumericRateSeries(distanceSeries, elapsedSeries, helpers)
  const derivedHeading = deriveHeadingSeries(courseSeries, helpers)
  const derivedGradient = useLegacyGpxDerivations
    ? deriveLegacyGradientSeries(directMetrics.elevation, distanceSeries, helpers)
    : deriveGradientSeries(directMetrics.elevation, distanceSeries, helpers)
  const derivedVerticalSpeed = deriveNumericRateSeries(directMetrics.elevation, elapsedSeries, helpers)
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
      gradient: combineSeriesPreferDerived(derivedGradient, directMetrics.gradient),
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
    },
  }
}

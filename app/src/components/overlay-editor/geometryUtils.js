/**
 * Provides overlay editor helpers for geometry utils.
 */

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} min - Lower bound used by the calculation.
 * @param {*} max - Upper bound used by the calculation.
 * @returns {number} Result produced by the helper.
 */
function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Builds fallback route.
 *
 * @param {*} width - Numeric width value.
 * @param {*} height - Numeric height value.
 * @returns {*} Derived data structure for downstream use.
 */
function buildFallbackRoute(width, height) {
  return [
    [width * 0.12, height * 0.82],
    [width * 0.3, height * 0.64],
    [width * 0.46, height * 0.72],
    [width * 0.64, height * 0.3],
    [width * 0.84, height * 0.18],
  ]
}

/**
 * Handles fit points to widget.
 *
 * @param {*} points - Cartesian points used to build geometry.
 * @param {*} width - Numeric width value.
 * @param {*} height - Numeric height value.
 * @param {*} insetPx - Value for inset px.
 * @param {*} invertY - Value for invert y.
 * @returns {*} Result produced by the helper.
 */
function fitPointsToWidget(points, width, height, insetPx, invertY = true) {
  if (!points.length) {
    return []
  }

  const minX = Math.min(...points.map(([x]) => x))
  const maxX = Math.max(...points.map(([x]) => x))
  const minY = Math.min(...points.map(([, y]) => y))
  const maxY = Math.max(...points.map(([, y]) => y))
  const safeInset = Math.min(
    Math.max(Number(insetPx) || 0, 0),
    Math.min(width, height) * 0.45,
  )
  const innerWidth = Math.max(width - safeInset * 2, 1)
  const innerHeight = Math.max(height - safeInset * 2, 1)
  const spanX = Math.max(maxX - minX, 0.000001)
  const spanY = Math.max(maxY - minY, 0.000001)
  const scale = Math.min(innerWidth / spanX, innerHeight / spanY)
  const offsetX = (width - spanX * scale) / 2
  const offsetY = (height - spanY * scale) / 2

  return points.map(([x, y]) => {
    const fittedX = (x - minX) * scale + offsetX
    let fittedY = (y - minY) * scale + offsetY
    if (invertY) {
      fittedY = height - fittedY
    }
    return [fittedX, fittedY]
  })
}

/**
 * Handles route geometry inset px.
 *
 * @param {*} widgetWidth - Numeric widget width value.
 * @param {*} widgetHeight - Numeric widget height value.
 * @param {*} lineWidth - Numeric line width value.
 * @param {*} completedLineWidth - Numeric completed line width value.
 * @param {*} markerSize - Numeric marker size value.
 * @returns {*} Result produced by the helper.
 */
function routeGeometryInsetPx(
  widgetWidth,
  widgetHeight,
  lineWidth,
  completedLineWidth,
  markerSize,
) {
  const safeWidth = Number(lineWidth) || 0
  const safeCompletedWidth = Number(completedLineWidth) || 0
  const safeMarkerSize = Number(markerSize) || 0
  const lineInset = Math.max(safeWidth, safeCompletedWidth) * 0.5
  return Math.min(
    Math.max(safeMarkerSize, lineInset) + 1,
    Math.min(widgetWidth, widgetHeight) * 0.45,
  )
}

/**
 * Handles simplify route samples.
 *
 * @param {*} samples - Value for samples.
 * @param {*} tolerance - Simplification tolerance applied to geometry.
 * @returns {*} Result produced by the helper.
 */
function simplifyRouteSamples(samples, tolerance) {
  if (samples.length <= 2 || tolerance <= 0) {
    return samples
  }

  const perpendicularDistance = (point, start, end) => {
    const [x0, y0] = point.point
    const [x1, y1] = start.point
    const [x2, y2] = end.point
    const dx = x2 - x1
    const dy = y2 - y1
    if (Math.abs(dx) <= Number.EPSILON && Math.abs(dy) <= Number.EPSILON) {
      return Math.hypot(x0 - x1, y0 - y1)
    }
    return Math.abs(dy * x0 - dx * y0 + x2 * y1 - y2 * x1) / Math.hypot(dx, dy)
  }

  let maxDistance = 0
  let splitIndex = 0
  for (let index = 1; index < samples.length - 1; index += 1) {
    const distance = perpendicularDistance(
      samples[index],
      samples[0],
      samples[samples.length - 1],
    )
    if (distance > maxDistance) {
      maxDistance = distance
      splitIndex = index
    }
  }

  if (maxDistance <= tolerance) {
    return [samples[0], samples[samples.length - 1]]
  }

  const left = simplifyRouteSamples(samples.slice(0, splitIndex + 1), tolerance)
  const right = simplifyRouteSamples(samples.slice(splitIndex), tolerance)
  return [...left.slice(0, -1), ...right]
}

/**
 * Handles downsample route samples.
 *
 * @param {*} samples - Value for samples.
 * @param {*} targetCount - Value for target count.
 * @returns {*} Result produced by the helper.
 */
function downsampleRouteSamples(samples, targetCount) {
  if (samples.length <= targetCount || targetCount < 3) {
    return samples
  }

  const bucketSize = (samples.length - 2) / (targetCount - 2)
  const sampled = [samples[0]]
  let a = 0

  for (let bucketIndex = 0; bucketIndex < targetCount - 2; bucketIndex += 1) {
    const avgStart = Math.floor((bucketIndex + 1) * bucketSize) + 1
    const avgEnd = Math.min(
      samples.length,
      Math.floor((bucketIndex + 2) * bucketSize) + 1,
    )
    const avgRangeStart = Math.min(avgStart, Math.max(avgEnd - 1, 0))
    const avgRange = samples.slice(avgRangeStart, avgEnd)
    const average =
      avgRange.length > 0
        ? {
            x:
              avgRange.reduce((sum, sample) => sum + sample.point[0], 0) /
              avgRange.length,
            y:
              avgRange.reduce((sum, sample) => sum + sample.point[1], 0) /
              avgRange.length,
          }
        : {
            x: samples[samples.length - 1].point[0],
            y: samples[samples.length - 1].point[1],
          }

    const rangeStart = Math.floor(bucketIndex * bucketSize) + 1
    const rangeEnd = Math.min(
      samples.length - 1,
      Math.floor((bucketIndex + 1) * bucketSize) + 1,
    )
    const candidateStart = Math.min(rangeStart, samples.length - 2)
    const candidateEnd = Math.max(candidateStart + 1, rangeEnd)

    let nextA = candidateStart
    let maxArea = -1
    for (
      let candidateIndex = candidateStart;
      candidateIndex < candidateEnd;
      candidateIndex += 1
    ) {
      const pointA = samples[a].point
      const pointB = samples[candidateIndex].point
      const area =
        Math.abs(
          (pointA[0] - average.x) * (pointB[1] - pointA[1]) -
            (pointA[0] - pointB[0]) * (average.y - pointA[1]),
        ) * 0.5
      if (area > maxArea) {
        maxArea = area
        nextA = candidateIndex
      }
    }

    a = nextA
    sampled.push(samples[a])
  }

  sampled.push(samples[samples.length - 1])
  return sampled
}

/**
 * Normalizes route geometry.
 *
 * @param {*} samples - Value for samples.
 * @param {*} width - Numeric width value.
 * @param {*} height - Numeric height value.
 * @param {*} targetDensity - Value for target density.
 * @param {*} simplifyTolerancePx - Value for simplify tolerance px.
 * @param {*} lineWidth - Numeric line width value.
 * @param {*} completedLineWidth - Numeric completed line width value.
 * @param {*} markerSize - Numeric marker size value.
 * @returns {object} Derived data structure for downstream use.
 */
export function normalizeRouteGeometry(
  samples,
  width,
  height,
  targetDensity = 1,
  simplifyTolerancePx = 1,
  lineWidth = 6,
  completedLineWidth = 6,
  markerSize = 18,
) {
  const validSamples = samples.filter(
    (sample) =>
      Array.isArray(sample?.point) &&
      Number.isFinite(sample.point[0]) &&
      Number.isFinite(sample.point[1]),
  )

  if (validSamples.length < 2) {
    const fallbackPoints = buildFallbackRoute(width, height)
    return {
      points: fallbackPoints,
      progressValues: fallbackPoints.map((_, index) =>
        fallbackPoints.length > 1 ? index / (fallbackPoints.length - 1) : 0,
      ),
    }
  }

  const validPoints = validSamples.map((sample) => sample.point)
  const latitudes = validPoints.map(([latitude]) => latitude)
  const meanLatitude =
    latitudes.reduce((sum, latitude) => sum + latitude, 0) / latitudes.length
  const meanLatitudeRadians = meanLatitude * (Math.PI / 180)
  const projectedPoints = validPoints.map(([latitude, longitude]) => [
    longitude * Math.cos(meanLatitudeRadians),
    latitude,
  ])
  const fitted = fitPointsToWidget(
    projectedPoints,
    width,
    height,
    routeGeometryInsetPx(
      width,
      height,
      lineWidth,
      completedLineWidth,
      markerSize,
    ),
    true,
  )
  const fittedSamples = validSamples.map((sample, index) => ({
    point: fitted[index],
    progress: Number.isFinite(sample.progress)
      ? clamp(sample.progress, 0, 1)
      : 0,
  }))
  const safeTargetDensity = clamp(Number(targetDensity) || 1, 0.1, 2)
  const targetCount = Math.max(
    2,
    Math.min(fittedSamples.length, Math.round(width * safeTargetDensity)),
  )
  const downsampled = downsampleRouteSamples(fittedSamples, targetCount)
  const simplified = simplifyRouteSamples(
    downsampled,
    Math.max(Number(simplifyTolerancePx) || 1, 0.05),
  )

  return {
    points: simplified.map((sample) => sample.point),
    progressValues: simplified.map((sample) => sample.progress),
  }
}

/**
 * Normalizes route points.
 *
 * @param {*} points - Cartesian points used to build geometry.
 * @param {*} width - Numeric width value.
 * @param {*} height - Numeric height value.
 * @param {*} _padding - Numeric padding value.
 * @returns {*} Derived data structure for downstream use.
 */
export function normalizeRoutePoints(points, width, height, _padding = 18) {
  return normalizeRouteGeometry(
    points.map((point, index) => ({
      point,
      progress: points.length > 1 ? index / (points.length - 1) : 0,
    })),
    width,
    height,
    1,
    1,
    6,
    6,
    18,
  ).points
}

/**
 * Builds widget transform.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.scale - Value for scale.
 * @param {*} options.rotation - Value for rotation.
 * @returns {*} Derived data structure for downstream use.
 */
export function buildWidgetTransform({ scale = 1, rotation = 0 }) {
  const transforms = []

  if (rotation) {
    transforms.push(`rotate(${rotation}deg)`)
  }

  if (scale !== 1) {
    transforms.push(`scale(${scale})`)
  }

  return transforms.length ? transforms.join(' ') : undefined
}

/**
 * Normalizes elevation points.
 *
 * @param {*} values - Input values processed by the helper.
 * @param {*} width - Numeric width value.
 * @param {*} height - Numeric height value.
 * @param {*} padding - Numeric padding value.
 * @param {*} verticalScale - Value for vertical scale.
 * @param {*} progressValues - Value for progress values.
 * @param {*} targetDensity - Value for target density.
 * @param {*} simplifyTolerancePx - Value for simplify tolerance px.
 * @returns {object} Derived data structure for downstream use.
 */
export function normalizeElevationGeometry(
  values,
  width,
  height,
  margin = 0,
  verticalScale = 1,
  progressValues = [],
  targetDensity = 0.75,
  simplifyTolerancePx = 1,
) {
  const samples = values.reduce((result, value, index) => {
    if (!Number.isFinite(value)) {
      return result
    }

    const progressValue = Number(progressValues[index])
    result.push({
      progress: Number.isFinite(progressValue)
        ? clamp(progressValue, 0, 1)
        : values.length > 1
          ? index / (values.length - 1)
          : 0,
      value: Number(value),
    })
    return result
  }, [])

  if (!samples.length) {
    const fallbackPadding = 18
    const fallbackPoints = [
      [fallbackPadding, height - fallbackPadding],
      [width * 0.32, height * 0.55],
      [width * 0.62, height * 0.36],
      [width - fallbackPadding, height * 0.48],
    ]
    return {
      points: fallbackPoints,
      progressValues: fallbackPoints.map((_, index) =>
        fallbackPoints.length > 1 ? index / (fallbackPoints.length - 1) : 0,
      ),
    }
  }

  const safeMargin = Number.isFinite(Number(margin)) ? Number(margin) : 0
  const innerWidth = Math.max(width * (1 - 2 * safeMargin), 1)
  const innerHeight = Math.max(height * (1 - 2 * safeMargin), 1)
  const safeVerticalScale = clamp(Number(verticalScale) || 1, 0.2, 4)
  const safeTargetDensity = clamp(Number(targetDensity) || 0.75, 0.1, 2)
  const safeSimplifyTolerance = clamp(Number(simplifyTolerancePx) || 0, 0, 8)

  const smoothElevationSamples = (inputSamples) => {
    const coefficients = [-36, 9, 44, 69, 84, 89, 84, 69, 44, 9, -36]
    const radius = Math.floor(coefficients.length / 2)

    return inputSamples.map((sample, index) => {
      let total = 0
      let coefficientTotal = 0

      for (let offset = -radius; offset <= radius; offset += 1) {
        const neighborIndex = index + offset
        if (neighborIndex < 0 || neighborIndex >= inputSamples.length) {
          continue
        }

        const neighborValue = inputSamples[neighborIndex].value
        if (!Number.isFinite(neighborValue)) {
          continue
        }

        const coefficient = coefficients[offset + radius]
        total += neighborValue * coefficient
        coefficientTotal += coefficient
      }

      return {
        ...sample,
        value:
          Math.abs(coefficientTotal) <= Number.EPSILON
            ? sample.value
            : total / coefficientTotal,
        preserve: index === 0 || index === inputSamples.length - 1,
      }
    })
  }

  const downsampleElevationSamples = (inputSamples, targetCount) => {
    if (inputSamples.length <= targetCount || targetCount < 3) {
      return inputSamples.map((sample, index) => ({
        ...sample,
        preserve: index === 0 || index === inputSamples.length - 1,
      }))
    }

    const smoothedSamples = smoothElevationSamples(inputSamples)
    const lastIndex = smoothedSamples.length - 1
    const selectedSamples = []

    for (let sampleIndex = 0; sampleIndex < targetCount; sampleIndex += 1) {
      const sourceIndex = Math.round(
        (sampleIndex * lastIndex) / Math.max(targetCount - 1, 1),
      )
      const nextSample = smoothedSamples[Math.min(sourceIndex, lastIndex)]
      if (
        selectedSamples.length > 0 &&
        selectedSamples[selectedSamples.length - 1].progress ===
          nextSample.progress
      ) {
        continue
      }
      selectedSamples.push(nextSample)
    }

    if (selectedSamples.length === 1 && smoothedSamples.length > 1) {
      selectedSamples.push(smoothedSamples[smoothedSamples.length - 1])
    }

    return selectedSamples
  }

  const simplifyProjectedPointsSegment = (inputPoints, tolerance) => {
    if (inputPoints.length <= 2 || tolerance <= 0) {
      return inputPoints
    }

    const perpendicularDistance = (point, start, end) => {
      const [x0, y0] = point.point
      const [x1, y1] = start.point
      const [x2, y2] = end.point
      const dx = x2 - x1
      const dy = y2 - y1
      if (Math.abs(dx) <= Number.EPSILON && Math.abs(dy) <= Number.EPSILON) {
        return Math.hypot(x0 - x1, y0 - y1)
      }
      return (
        Math.abs(dy * x0 - dx * y0 + x2 * y1 - y2 * x1) / Math.hypot(dx, dy)
      )
    }

    let maxDistance = 0
    let splitIndex = 0
    for (let index = 1; index < inputPoints.length - 1; index += 1) {
      const distance = perpendicularDistance(
        inputPoints[index],
        inputPoints[0],
        inputPoints[inputPoints.length - 1],
      )
      if (distance > maxDistance) {
        maxDistance = distance
        splitIndex = index
      }
    }

    if (maxDistance <= tolerance) {
      return [inputPoints[0], inputPoints[inputPoints.length - 1]]
    }

    const left = simplifyProjectedPointsSegment(
      inputPoints.slice(0, splitIndex + 1),
      tolerance,
    )
    const right = simplifyProjectedPointsSegment(
      inputPoints.slice(splitIndex),
      tolerance,
    )
    return [...left.slice(0, -1), ...right]
  }

  const simplifyProjectedPoints = (inputPoints, tolerance) => {
    if (inputPoints.length <= 2 || tolerance <= 0) {
      return inputPoints
    }

    const preservedIndexes = inputPoints.reduce((result, point, index) => {
      if (point.preserve) result.push(index)
      return result
    }, [])
    if (preservedIndexes.length >= 2) {
      const result = []
      for (
        let windowIndex = 0;
        windowIndex < preservedIndexes.length - 1;
        windowIndex += 1
      ) {
        const start = preservedIndexes[windowIndex]
        const end = preservedIndexes[windowIndex + 1]
        const simplifiedSegment = simplifyProjectedPointsSegment(
          inputPoints.slice(start, end + 1),
          tolerance,
        )
        if (result.length === 0) {
          result.push(...simplifiedSegment)
        } else {
          result.push(...simplifiedSegment.slice(1))
        }
      }
      return result
    }

    return simplifyProjectedPointsSegment(inputPoints, tolerance)
  }

  const targetCount = Math.max(
    2,
    Math.min(samples.length, Math.round(width * safeTargetDensity)),
  )
  const downsampledSamples = downsampleElevationSamples(samples, targetCount)
  const usableValues = downsampledSamples.map((sample) => sample.value)
  const minimum = Math.min(...usableValues)
  const maximum = Math.max(...usableValues)
  const amplitude = Math.max(maximum - minimum, 1e-9)

  const projectedPoints = downsampledSamples.map((sample) => {
    const progress = Number.isFinite(sample.progress)
      ? clamp(sample.progress, 0, 1)
      : 0
    const x = width * safeMargin + innerWidth * progress
    const normalized =
      amplitude <= 0 ? 0.5 : (sample.value - minimum) / amplitude
    const centered = clamp((normalized - 0.5) * safeVerticalScale + 0.5, 0, 1)
    const y = height - (height * safeMargin + innerHeight * centered)
    return {
      point: [x, y],
      progress,
      preserve: sample.preserve === true,
    }
  })

  const simplified = simplifyProjectedPoints(
    projectedPoints,
    safeSimplifyTolerance,
  )

  return {
    points: simplified.map(({ point }) => point),
    progressValues: simplified.map(({ progress }) => progress),
  }
}

/**
 * Handles points to svg.
 *
 * @param {*} points - Cartesian points used to build geometry.
 * @returns {*} Result produced by the helper.
 */
export function pointsToSvg(points) {
  return points.map(([x, y]) => `${x},${y}`).join(' ')
}

/**
 * Returns point at progress.
 *
 * @param {*} points - Cartesian points used to build geometry.
 * @param {*} progress01 - Normalized progress value between 0 and 1.
 * @returns {*} Requested value or structure.
 */
export function getPointAtProgress(points, progress01) {
  if (!points.length) {
    return null
  }

  if (points.length === 1) {
    return points[0]
  }

  const clampedProgress = clamp(Number(progress01) || 0, 0, 1)
  const scaledIndex = clampedProgress * (points.length - 1)
  const startIndex = Math.floor(scaledIndex)
  const endIndex = Math.min(startIndex + 1, points.length - 1)
  const mix = scaledIndex - startIndex
  const startPoint = points[startIndex]
  const endPoint = points[endIndex]

  if (!startPoint || !endPoint) {
    return points[Math.min(startIndex, points.length - 1)] || null
  }

  return [
    startPoint[0] + (endPoint[0] - startPoint[0]) * mix,
    startPoint[1] + (endPoint[1] - startPoint[1]) * mix,
  ]
}

/**
 * Returns point at metric progress.
 *
 * @param {*} points - Cartesian points used to build geometry.
 * @param {*} progressValues - Value for progress values.
 * @param {*} targetProgress - Value for target progress.
 * @returns {*} Requested value or structure.
 */
export function getPointAtMetricProgress(
  points,
  progressValues,
  targetProgress,
) {
  const result = getPointAtMetricProgressWithIndex(
    points,
    progressValues,
    targetProgress,
  )

  return result ? result.point : null
}

/**
 * Returns point and right-side segment index at metric progress.
 *
 * @param {*} points - Cartesian points used to build geometry.
 * @param {*} progressValues - Value for progress values.
 * @param {*} targetProgress - Value for target progress.
 * @returns {*} Requested value or structure.
 */
export function getPointAtMetricProgressWithIndex(
  points,
  progressValues,
  targetProgress,
) {
  if (
    !Array.isArray(points) ||
    !Array.isArray(progressValues) ||
    !points.length
  ) {
    return null
  }

  const safeTargetProgress = clamp(Number(targetProgress) || 0, 0, 1)
  let firstValidIndex = -1
  let lastValidIndex = -1

  for (let index = 0; index < points.length; index += 1) {
    if (
      points[index] &&
      Number.isFinite(points[index][0]) &&
      Number.isFinite(points[index][1]) &&
      Number.isFinite(progressValues[index])
    ) {
      firstValidIndex = index
      break
    }
  }

  if (firstValidIndex === -1) {
    const point = getPointAtProgress(points, safeTargetProgress)
    return point ? { index: 0, point } : null
  }

  for (let index = points.length - 1; index >= 0; index -= 1) {
    if (
      points[index] &&
      Number.isFinite(points[index][0]) &&
      Number.isFinite(points[index][1]) &&
      Number.isFinite(progressValues[index])
    ) {
      lastValidIndex = index
      break
    }
  }

  if (safeTargetProgress <= progressValues[firstValidIndex]) {
    return {
      index: Math.min(firstValidIndex + 1, points.length - 1),
      point: points[firstValidIndex],
    }
  }

  if (safeTargetProgress >= progressValues[lastValidIndex]) {
    return { index: lastValidIndex, point: points[lastValidIndex] }
  }

  let leftIndex = firstValidIndex
  let rightIndex = firstValidIndex

  for (let index = firstValidIndex + 1; index <= lastValidIndex; index += 1) {
    const nextProgress = Number(progressValues[index])
    if (!Number.isFinite(nextProgress)) {
      continue
    }

    if (nextProgress >= safeTargetProgress) {
      rightIndex = index
      break
    }

    leftIndex = index
  }

  const leftProgress = Number(progressValues[leftIndex])
  const rightProgress = Number(progressValues[rightIndex])
  const leftPoint = points[leftIndex]
  const rightPoint = points[rightIndex]

  if (
    !Number.isFinite(leftProgress) ||
    !Number.isFinite(rightProgress) ||
    !leftPoint ||
    !rightPoint
  ) {
    return null
  }

  if (rightIndex === leftIndex || rightProgress === leftProgress) {
    return { index: rightIndex, point: leftPoint }
  }

  const ratio =
    (safeTargetProgress - leftProgress) / (rightProgress - leftProgress)

  return {
    index: rightIndex,
    point: [
      leftPoint[0] + (rightPoint[0] - leftPoint[0]) * ratio,
      leftPoint[1] + (rightPoint[1] - leftPoint[1]) * ratio,
    ],
  }
}

/**
 * Handles area to svg.
 *
 * @param {*} points - Cartesian points used to build geometry.
 * @param {*} width - Numeric width value.
 * @param {*} height - Numeric height value.
 * @param {*} padding - Numeric padding value.
 * @returns {*} Result produced by the helper.
 */
export function areaToSvg(points, _width, height, padding = 18) {
  if (!points.length) return ''
  const baseline = Number.isFinite(padding) ? height - padding : height
  return [
    `${points[0][0]},${baseline}`,
    ...points.map(([x, y]) => `${x},${y}`),
    `${points[points.length - 1][0]},${baseline}`,
  ].join(' ')
}

/**
 * Returns completed index.
 *
 * @param {*} totalPoints - Value for total points.
 * @param {*} sampleIndex - Sample index within the activity series.
 * @param {*} progress01 - Normalized progress value between 0 and 1.
 * @returns {*} Requested value or structure.
 */
export function getCompletedIndex(totalPoints, sampleIndex, progress01) {
  if (totalPoints <= 1) return 0

  if (Number.isFinite(progress01)) {
    return clamp(Math.floor(progress01 * (totalPoints - 1)), 0, totalPoints - 1)
  }

  return clamp(sampleIndex, 0, totalPoints - 1)
}

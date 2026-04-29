function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

function buildFallbackRoute(width, height) {
  return [
    [width * 0.12, height * 0.82],
    [width * 0.3, height * 0.64],
    [width * 0.46, height * 0.72],
    [width * 0.64, height * 0.3],
    [width * 0.84, height * 0.18],
  ]
}

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
    Math.max(clamp(Number(simplifyTolerancePx) || 1, 0, 8), 0.05),
  )

  return {
    points: simplified.map((sample) => sample.point),
    progressValues: simplified.map((sample) => sample.progress),
  }
}

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

export function normalizeElevationPoints(
  values,
  width,
  height,
  padding = 18,
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
        : null,
      value: Number(value),
    })
    return result
  }, [])

  if (!samples.length) {
    return [
      [padding, height - padding],
      [width * 0.32, height * 0.55],
      [width * 0.62, height * 0.36],
      [width - padding, height * 0.48],
    ]
  }

  const usableValues = samples.map((sample) => sample.value)
  const minimum = Math.min(...usableValues)
  const maximum = Math.max(...usableValues)
  const amplitude = Math.max(maximum - minimum, 1)
  const usableWidth = Math.max(width - padding * 2, 1)
  const hasUsableProgress = samples.some((sample) =>
    Number.isFinite(sample.progress),
  )
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
  const downsampledFallbackStep =
    downsampledSamples.length > 1
      ? usableWidth / (downsampledSamples.length - 1)
      : 0
  const projectedPoints = downsampledSamples.map((sample, index) => {
    const x = hasUsableProgress
      ? padding + (sample.progress ?? 0) * usableWidth
      : padding + index * downsampledFallbackStep
    const normalized =
      amplitude <= 0 ? 0.5 : (sample.value - minimum) / amplitude
    const centered = clamp((normalized - 0.5) * safeVerticalScale + 0.5, 0, 1)
    const y = height - padding - centered * (height - padding * 2)
    return {
      point: [x, y],
      progress: sample.progress,
      preserve: sample.preserve === true,
    }
  })

  return simplifyProjectedPoints(projectedPoints, safeSimplifyTolerance).map(
    ({ point }) => point,
  )
}

export function pointsToSvg(points) {
  return points.map(([x, y]) => `${x},${y}`).join(' ')
}

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

export function getPointAtMetricProgress(
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
    return getPointAtProgress(points, safeTargetProgress)
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
    return points[firstValidIndex]
  }

  if (safeTargetProgress >= progressValues[lastValidIndex]) {
    return points[lastValidIndex]
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
    return leftPoint
  }

  const ratio =
    (safeTargetProgress - leftProgress) / (rightProgress - leftProgress)

  return [
    leftPoint[0] + (rightPoint[0] - leftPoint[0]) * ratio,
    leftPoint[1] + (rightPoint[1] - leftPoint[1]) * ratio,
  ]
}

export function getPointAtX(points, targetX) {
  if (!points.length) {
    return null
  }

  if (points.length === 1) {
    return points[0]
  }

  const safeTargetX = Number(targetX)
  if (!Number.isFinite(safeTargetX)) {
    return null
  }

  if (safeTargetX <= points[0][0]) {
    return points[0]
  }

  const lastPoint = points[points.length - 1]
  if (safeTargetX >= lastPoint[0]) {
    return lastPoint
  }

  for (let index = 1; index < points.length; index += 1) {
    const leftPoint = points[index - 1]
    const rightPoint = points[index]
    if (!leftPoint || !rightPoint) {
      continue
    }

    if (rightPoint[0] < safeTargetX) {
      continue
    }

    const deltaX = rightPoint[0] - leftPoint[0]
    if (!Number.isFinite(deltaX) || deltaX === 0) {
      return rightPoint
    }

    const ratio = (safeTargetX - leftPoint[0]) / deltaX
    return [
      leftPoint[0] + (rightPoint[0] - leftPoint[0]) * ratio,
      leftPoint[1] + (rightPoint[1] - leftPoint[1]) * ratio,
    ]
  }

  return lastPoint
}

export function areaToSvg(points, width, height, padding = 18) {
  if (!points.length) return ''
  return [
    `${padding},${height - padding}`,
    ...points.map(([x, y]) => `${x},${y}`),
    `${width - padding},${height - padding}`,
  ].join(' ')
}

export function getCompletedIndex(totalPoints, sampleIndex, progress01) {
  if (totalPoints <= 1) return 0

  if (Number.isFinite(progress01)) {
    return clamp(Math.floor(progress01 * (totalPoints - 1)), 0, totalPoints - 1)
  }

  return clamp(sampleIndex, 0, totalPoints - 1)
}

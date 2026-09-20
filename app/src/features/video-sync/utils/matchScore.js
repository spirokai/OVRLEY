export const VIDEO_SYNC_MIN_MEANINGFUL_QUALITY = 0.2

/**
 * Calculates a bounded robust quality for the distance outside an event's
 * ideal offset interval. The Cauchy kernel gives the interval a flat maximum
 * and lets near misses decline continuously without allowing distant outliers
 * to dominate the fit.
 *
 * @param {number} residualSeconds Distance outside the ideal interval.
 * @param {number} timingScaleSeconds Timing uncertainty for this landmark type.
 * @returns {number} Match quality from zero to one.
 */
export function calculateTimingQuality(residualSeconds, timingScaleSeconds) {
  const normalizedResidual = residualSeconds / timingScaleSeconds
  return 1 / (1 + normalizedResidual ** 2)
}

/**
 * Calculates the normalized robust score for one complete assignment.
 * Unassigned landmarks contribute zero through the eligible-count denominator.
 *
 * @param {{residual: number, quality: number}[]} matches Assigned landmark/event pairs.
 * @param {number} eligibleCount Number of eligible video landmarks.
 * @returns {{matchedCount: number, eligibleCount: number, totalQuality: number, coverage: number, matchScore: number, totalResidualSeconds: number}} Score diagnostics.
 */
export function calculateMatchScore(matches, eligibleCount) {
  const meaningfulMatches = matches.filter(({ quality }) => quality >= VIDEO_SYNC_MIN_MEANINGFUL_QUALITY)
  const matchedCount = meaningfulMatches.length
  const totalQuality = matches.reduce((total, { quality }) => total + quality, 0)
  const coverage = eligibleCount === 0 ? 0 : matchedCount / eligibleCount
  const matchScore = eligibleCount === 0 ? 0 : Math.round((100 * totalQuality) / eligibleCount)

  return {
    matchedCount,
    eligibleCount,
    totalQuality,
    coverage,
    matchScore,
    totalResidualSeconds: meaningfulMatches.reduce((total, { residual }) => total + residual, 0),
  }
}

/**
 * Compares two ordinary candidates using the documented deterministic order.
 *
 * @param {{matchScore: number, matchedCount: number, totalResidualSeconds: number, offset: number}} left Candidate to compare.
 * @param {{matchScore: number, matchedCount: number, totalResidualSeconds: number, offset: number}} right Candidate to compare.
 * @returns {number} A negative value when left is stronger, zero when tied, or a positive value otherwise.
 */
export function compareCandidateStrength(left, right) {
  return (
    right.matchScore - left.matchScore ||
    right.matchedCount - left.matchedCount ||
    left.totalResidualSeconds - right.totalResidualSeconds ||
    left.offset - right.offset
  )
}

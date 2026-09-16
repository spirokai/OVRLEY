/**
 * Calculates the absolute normalized likelihood for one landmark assignment.
 *
 * @param {number[]} residuals Matched landmark timing residuals in seconds.
 * @param {number} eligibleCount Number of eligible video landmarks.
 * @returns {{matchedCount: number, eligibleCount: number, chiSquare: number, timingLikelihood: number, coverage: number, matchScore: number, totalResidualSeconds: number}} Score diagnostics.
 */
export function calculateMatchScore(residuals, eligibleCount) {
  const matchedCount = residuals.length
  const chiSquare = residuals.reduce((total, residual) => total + (residual / 2) ** 2, 0)
  const timingLikelihood = matchedCount === 0 ? 0 : Math.exp(-chiSquare / (2 * matchedCount))
  const coverage = eligibleCount === 0 ? 0 : matchedCount / eligibleCount
  const matchScore = Math.round(100 * coverage * timingLikelihood)

  return {
    matchedCount,
    eligibleCount,
    chiSquare,
    timingLikelihood,
    coverage,
    matchScore,
    totalResidualSeconds: residuals.reduce((total, residual) => total + residual, 0),
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

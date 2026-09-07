/** @param {object} left - Job inputs. @param {object} right - Current inputs. @returns {boolean} Whether all source revisions match. */
export function sameVisualSyncInputs(left, right) {
  return (
    left.video_identity === right.video_identity &&
    left.video_revision === right.video_revision &&
    left.activity_identity === right.activity_identity &&
    left.activity_revision === right.activity_revision
  )
}

/** @param {number} seconds - Source-relative timestamp. @returns {string} Seconds at display precision. */
export function formatAnalyzedSeconds(seconds) {
  return seconds.toFixed(1)
}

/** @param {object} snapshot - Canonical job snapshot. @returns {string} Job presentation status. */
export function visualSyncStatus(snapshot) {
  if (snapshot.terminal === null) return snapshot.stage
  switch (snapshot.terminal.kind) {
    case 'error':
      return 'error'
    case 'cancelled':
      return 'cancelled'
    case 'result':
      return snapshot.terminal.result.accepted ? 'matched' : 'no_match'
    default:
      throw new Error(`Unknown visual sync terminal kind: ${snapshot.terminal.kind}`)
  }
}

/** @param {object} candidate - Canonical candidate. @param {boolean} applied - Explicit selection state. @param {function} t - Translator. @returns {object} Display copy. */
export function describeVisualSyncCandidate(candidate, applied, t) {
  return {
    offset_seconds: candidate.offset_seconds,
    accepted: candidate.accepted,
    applied,
    action: t(applied ? 'syncDoctor.applied' : candidate.accepted ? 'syncDoctor.apply' : 'syncDoctor.diagnostic', {
      seconds: candidate.offset_seconds.toFixed(2),
    }),
    evidence: t('syncDoctor.evidence', {
      correlation: candidate.correlation === null ? t('syncDoctor.unavailable') : candidate.correlation.toFixed(3),
      nomination: candidate.nomination_correlation.toFixed(3),
      margin: candidate.nomination_margin.toFixed(3),
    }),
    support: t('syncDoctor.support', {
      seconds: candidate.observed_seconds.toFixed(1),
      fraction: (candidate.observed_fraction * 100).toFixed(1),
      retained: (candidate.retained_video_observation_fraction * 100).toFixed(1),
    }),
    sections: candidate.sections.map((section) => ({
      video_start_seconds: section.video_start_seconds,
      label: t('syncDoctor.section', {
        start: section.video_start_seconds.toFixed(0),
        end: section.video_end_seconds.toFixed(0),
        role: t(section.held_out ? 'syncDoctor.heldOut' : 'syncDoctor.nomination'),
        correlation: section.correlation === null ? t('syncDoctor.unavailable') : section.correlation.toFixed(3),
        agreement: t(section.agrees ? 'syncDoctor.agrees' : 'syncDoctor.disagrees'),
      }),
    })),
    rejection_reasons: candidate.rejection_reasons.map((reason) => t(`syncDoctor.reasons.${reason}`)),
  }
}

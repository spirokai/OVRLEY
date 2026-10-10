import i18next from 'i18next'
import { getZonedDateTimeParts } from '@/lib/time-format'
import { videoOverlapsActivity } from '@/lib/video-timing'

class VideoSyncError extends Error {
  constructor(key, message) {
    super(message)
    this.translationKey = key
  }
}

// External timestamps may be missing or unusable. Preserve the existing
// comparison clock: GPS is zoned; ambiguous local camera clock text is unchanged.
function parseSyncTimestamp(timestamp, source, timezone) {
  if (typeof timestamp !== 'string' || timestamp.trim() === '') return null
  try {
    let normalized
    if (source === 'gps') {
      const values = getZonedDateTimeParts(timestamp, timezone)
      normalized = `${values.year}-${values.month}-${values.day}T${values.hour}:${values.minute}:${values.second}`
    } else {
      normalized = timestamp
        .replace(/\.\d+(?=(?:Z|[+-]\d{2}:?\d{2}| UTC)?$)/, '')
        .replace(/(?:Z|[+-]\d{2}:?\d{2}| UTC)$/, '')
        .trim()
        .replace(' ', 'T')
    }
    const parsed = Date.parse(`${normalized}Z`)
    return Number.isFinite(parsed) ? parsed : null
  } catch (error) {
    if (error instanceof RangeError) return null
    throw error
  }
}

/**
 * Captures Apply Timezone: UTC when checked, local when unchecked. An automatic
 * unchecked selection captures the local interpretation.
 * @param {'utc'|'local'|null|undefined} mode Editor selection.
 * @returns {'utc'|'local'} Captured interpretation.
 */
export function captureVideoSyncTimezoneMode(mode) {
  if (mode === null || mode === undefined) return 'local'
  if (mode !== 'local' && mode !== 'utc') throw new Error('Video sync timezone mode must be local or utc')
  return mode
}

/**
 * Calculates a signed baseline independently of clip overlap. GPS and other
 * trusted sources keep absolute-time handling; camera/filename timestamps use
 * the selected interpretation. Interactive auto-sync resolves that selection
 * before batch calibration captures it.
 * @param {object} video Source metadata with creationTime and timeSource.
 * @param {object} activitySummary Activity syncTime, endTime and timezone.
 * @param {'utc'|'local'} timezoneMode Captured Apply Timezone interpretation.
 * @returns {{automaticOffsetSeconds: number, activityDurationSeconds: number}} Raw timing.
 */
export function calculateAutomaticVideoOffset(video, activitySummary, timezoneMode) {
  if (!video.creationTime) {
    throw new VideoSyncError('store.couldNotDetermineVideoCreationTime', 'Could not determine video creation time')
  }
  const timezone = activitySummary?.timezone
  if (!timezone) throw new VideoSyncError('store.timezoneIsRequiredForVideoSync', 'timezone is required for video sync')

  const ambiguous = video.timeSource === 'ffprobe' || video.timeSource === 'filename'
  const source = ambiguous && timezoneMode === 'local' ? 'ffprobe' : 'gps'
  const videoStart = parseSyncTimestamp(video.creationTime, source, timezone)
  const activityStart = parseSyncTimestamp(activitySummary.syncTime, 'gps', timezone)
  const activityEnd = parseSyncTimestamp(activitySummary.endTime, 'gps', timezone)
  if (videoStart === null || activityStart === null || activityEnd === null || activityEnd <= activityStart) {
    throw new VideoSyncError('store.invalidTimestampFormats', 'Invalid timestamp formats')
  }
  return {
    automaticOffsetSeconds: (videoStart - activityStart) / 1000,
    activityDurationSeconds: (activityEnd - activityStart) / 1000,
  }
}

/**
 * Resolves interactive sync state using the same baseline as batch calibration.
 * Ambiguous camera timestamps try both interpretations until a mode is selected;
 * prefer local when both overlap and retain an explicit selection.
 * A sole local match remains automatic for subsequent activity changes.
 * Unresolvable external timestamps or clips without positive activity overlap
 * use the warning/zero UI state; overlapping clips retain their signed offset.
 * @param {object} videoState Imported video state.
 * @param {object|null} activitySummary Activity summary.
 * @returns {object} Store sync fields ready to commit.
 */
export function resolveVideoSyncState(videoState, activitySummary) {
  const mode = captureVideoSyncTimezoneMode(videoState.videoSyncTimezoneMode)
  const ambiguous = videoState.importedVideoTimeSource === 'ffprobe' || videoState.importedVideoTimeSource === 'filename'
  const inferMode = ambiguous && (videoState.videoSyncTimezoneMode === null || videoState.videoSyncTimezoneMode === undefined)
  const modes = inferMode ? ['local', 'utc'] : [mode]
  try {
    const candidates = modes.map((timezoneMode) => {
      const { automaticOffsetSeconds, activityDurationSeconds } = calculateAutomaticVideoOffset(
        { creationTime: videoState.importedVideoCreationTime, timeSource: videoState.importedVideoTimeSource },
        activitySummary,
        timezoneMode,
      )
      return {
        timezoneMode,
        automaticOffsetSeconds,
        overlaps: videoOverlapsActivity({
          videoStart: automaticOffsetSeconds,
          videoDuration: videoState.importedVideoDuration,
          activityEnd: activityDurationSeconds,
        }),
      }
    })
    const matches = candidates.filter(({ overlaps }) => overlaps)
    const selected = matches[0] ?? candidates[0]
    const inferredLocalOnly = inferMode && matches.length === 1 && selected.timezoneMode === 'local'
    return {
      videoSyncOffsetSeconds: selected.overlaps ? selected.automaticOffsetSeconds : 0,
      videoSyncWarning: selected.overlaps ? null : i18next.t('store.videoCouldNotBeSyncedWithActivity', 'Video could not be synced with activity'),
      videoSyncTimezoneMode: ambiguous && !inferredLocalOnly ? selected.timezoneMode : null,
    }
  } catch (error) {
    if (!(error instanceof VideoSyncError)) throw error
    return {
      videoSyncOffsetSeconds: 0,
      videoSyncWarning: i18next.t(error.translationKey, error.message),
      videoSyncTimezoneMode: null,
    }
  }
}

/**
 * Captures the shared correction from the committed reference offset. A null
 * activity selects per-video embedded telemetry; a null reference explicitly
 * means zero correction. A present reference must have a resolvable baseline.
 * @param {object} options Calibration inputs from one synchronous snapshot.
 * @param {object|null} options.activitySummary Shared external activity summary, or null.
 * @param {object|null} options.referenceVideo Effective path, creationTime, timeSource and committedOffsetSeconds, or null.
 * @param {'utc'|'local'|null|undefined} options.timezoneMode Apply Timezone selection.
 * @returns {object} Serializable calibration, matching the Rust BatchCalibration contract.
 */
export function createBatchCalibration({ activitySummary, referenceVideo, timezoneMode }) {
  const mode = captureVideoSyncTimezoneMode(timezoneMode)
  if (activitySummary === null) return { mode: 'embeddedActivity' }
  if (referenceVideo === null) return { mode: 'externalActivity', timezoneMode: mode, correctionSeconds: 0, reference: null }
  if (!Number.isFinite(referenceVideo.committedOffsetSeconds)) throw new Error('Committed video sync offset must be a finite number')
  let baseline
  try {
    baseline = calculateAutomaticVideoOffset(referenceVideo, activitySummary, mode)
  } catch (error) {
    throw new Error(`Could not calibrate reference video: ${error.message}`)
  }
  const { automaticOffsetSeconds } = baseline
  return {
    mode: 'externalActivity',
    timezoneMode: mode,
    correctionSeconds: referenceVideo.committedOffsetSeconds - automaticOffsetSeconds,
    reference: {
      path: referenceVideo.path,
      creationTime: referenceVideo.creationTime,
      timeSource: referenceVideo.timeSource,
      committedOffsetSeconds: referenceVideo.committedOffsetSeconds,
      automaticOffsetSeconds,
    },
  }
}

/**
 * Evaluates automatic overlap using only the inspected video's timestamp.
 * Reference overrides and shared calibration belong to render submission.
 * Embedded telemetry is prepared later by its owning job; null overlap
 * explicitly means coverage has not yet been inspected.
 * @param {object} video Inspected source metadata with creationTime, timeSource and duration.
 * @param {object|null} activitySummary Shared external activity summary, or null.
 * @param {'utc'|'local'} timezoneMode Captured Apply Timezone interpretation.
 * @returns {{timing: object, hasPositiveOverlap: boolean|null}} Job timing and eligibility.
 */
export function resolveBatchVideoTiming(video, activitySummary, timezoneMode) {
  if (activitySummary === null) return { timing: { mode: 'embeddedActivity' }, hasPositiveOverlap: null }
  const { automaticOffsetSeconds, activityDurationSeconds } = calculateAutomaticVideoOffset(video, activitySummary, timezoneMode)
  return {
    timing: { mode: 'externalActivity', automaticOffsetSeconds },
    hasPositiveOverlap: videoOverlapsActivity({
      videoStart: automaticOffsetSeconds,
      videoDuration: video.duration,
      activityEnd: activityDurationSeconds,
    }),
  }
}

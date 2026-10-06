import i18next from 'i18next'
import { formatVideoCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
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
    const formatted = formatVideoCreationTime(timestamp, source, timezone)
    const normalized = formatted.trim().replace(' ', 'T')
    const parsed = Date.parse(normalized.endsWith('Z') ? normalized : `${normalized}Z`)
    return Number.isFinite(parsed) ? parsed : null
  } catch (error) {
    if (error instanceof RangeError) return null
    throw error
  }
}

/**
 * Captures Apply Timezone: UTC when checked, local when unchecked. Null or
 * omitted selection is the documented initial unchecked UI state.
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
 * the selected interpretation, never whichever interpretation overlaps.
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
 * Unresolvable external timestamps or clips without positive activity overlap
 * use the warning/zero UI state; overlapping clips retain their signed offset.
 * @param {object} videoState Imported video state.
 * @param {object|null} activitySummary Activity summary.
 * @returns {object} Store sync fields ready to commit.
 */
export function resolveVideoSyncState(videoState, activitySummary) {
  const mode = captureVideoSyncTimezoneMode(videoState.videoSyncTimezoneMode)
  const ambiguous = videoState.importedVideoTimeSource === 'ffprobe' || videoState.importedVideoTimeSource === 'filename'
  try {
    const { automaticOffsetSeconds, activityDurationSeconds } = calculateAutomaticVideoOffset(
      { creationTime: videoState.importedVideoCreationTime, timeSource: videoState.importedVideoTimeSource },
      activitySummary,
      mode,
    )
    const overlaps = videoOverlapsActivity({
      videoStart: automaticOffsetSeconds,
      videoDuration: videoState.importedVideoDuration,
      activityEnd: activityDurationSeconds,
    })
    return {
      videoSyncOffsetSeconds: overlaps ? automaticOffsetSeconds : 0,
      videoSyncWarning: overlaps ? null : i18next.t('store.videoCouldNotBeSyncedWithActivity', 'Video could not be synced with activity'),
      videoSyncTimezoneMode: ambiguous ? mode : null,
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
 * Adds the captured correction exactly once, then evaluates positive overlap.
 * A queued reference uses its captured effective timestamp/source, including
 * an editor override, so its offset remains the committed reference offset.
 * Embedded telemetry is prepared later by its owning job, with local offset
 * zero; null overlap explicitly means coverage has not yet been inspected.
 * @param {object} video Inspected source metadata with creationTime, timeSource and duration.
 * @param {object|null} activitySummary Shared external activity summary, or null.
 * @param {object} calibration Captured calibration from createBatchCalibration.
 * @param {boolean} [isReferenceVideo=false] Inspection identity matches the reference source. Path spellings are not file identities.
 * @returns {{timing: object, hasPositiveOverlap: boolean|null}} Job timing and eligibility.
 */
export function resolveBatchVideoTiming(video, activitySummary, calibration, isReferenceVideo = false) {
  if (calibration.mode === 'embeddedActivity') return { timing: { mode: 'embeddedActivity', offsetSeconds: 0 }, hasPositiveOverlap: null }
  const timestampSource = isReferenceVideo ? calibration.reference : video
  const { automaticOffsetSeconds, activityDurationSeconds } = calculateAutomaticVideoOffset(
    timestampSource,
    activitySummary,
    calibration.timezoneMode,
  )
  const offsetSeconds = automaticOffsetSeconds + calibration.correctionSeconds
  return {
    timing: { mode: 'externalActivity', automaticOffsetSeconds, offsetSeconds },
    hasPositiveOverlap: videoOverlapsActivity({ videoStart: offsetSeconds, videoDuration: video.duration, activityEnd: activityDurationSeconds }),
  }
}

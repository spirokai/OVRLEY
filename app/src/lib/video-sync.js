import i18next from 'i18next'
import { getZonedDateTimeParts } from '@/lib/time-format'

/**
 * Reports whether a video interval overlaps the activity interval.
 *
 * @param {object} options Timing inputs.
 * @param {number} options.videoStart Timeline second where the video interval starts.
 * @param {number} options.videoDuration Video interval duration in seconds.
 * @param {number} [options.activityEnd=Infinity] Activity interval end in seconds.
 * @returns {boolean} Whether the half-open intervals overlap.
 */
export function videoOverlapsActivity({ videoStart, videoDuration, activityEnd = Number.POSITIVE_INFINITY }) {
  return videoStart < activityEnd && videoStart + videoDuration > 0
}

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
 * the selected interpretation.
 * @param {object} video Source metadata with creationTime and timeSource.
 * @param {object} activitySummary Activity syncTime, endTime and timezone.
 * @param {'utc'|'local'} timezoneMode Captured Apply Timezone interpretation.
 * @returns {{automaticOffsetSeconds: number, activityDurationSeconds: number}} Raw timing.
 */
function calculateAutomaticVideoOffset(video, activitySummary, timezoneMode) {
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

function isAmbiguousTimeSource(timeSource) {
  return timeSource === 'ffprobe' || timeSource === 'filename'
}

/**
 * Resolves the automatic sync state of one clip.
 * Ambiguous camera timestamps try both interpretations until a mode is selected;
 * prefer local when both overlap and retain an explicit selection.
 * A sole local match remains automatic for subsequent activity changes.
 * Unresolvable external timestamps use the warning/zero UI state.
 * A clip outside the activity keeps its signed offset and reports that it has no activity coverage.
 * @param {object} video Imported video entity.
 * @param {object} activitySummary External activity summary.
 * @param {'utc'|'local'|null} sessionMode Explicit Apply Timezone selection, or null to infer.
 * @returns {{offsetSeconds: number, determined: boolean, syncWarning: string|null, timezoneMode: 'utc'|'local'|null}} Clip sync
 *   result; `determined` is false when the clip's time could not be resolved against the activity.
 */
export function resolveVideoSync(video, activitySummary, sessionMode) {
  const mode = captureVideoSyncTimezoneMode(sessionMode)
  const ambiguous = isAmbiguousTimeSource(video.timeSource)
  const inferMode = ambiguous && sessionMode === null
  const modes = inferMode ? ['local', 'utc'] : [mode]
  try {
    const candidates = modes.map((timezoneMode) => {
      const { automaticOffsetSeconds, activityDurationSeconds } = calculateAutomaticVideoOffset(video, activitySummary, timezoneMode)
      return {
        timezoneMode,
        automaticOffsetSeconds,
        overlaps: videoOverlapsActivity({
          videoStart: automaticOffsetSeconds,
          videoDuration: video.durationSeconds,
          activityEnd: activityDurationSeconds,
        }),
      }
    })
    const matches = candidates.filter(({ overlaps }) => overlaps)
    const selected = matches[0] ?? candidates[0]
    const inferredLocalOnly = inferMode && matches.length === 1 && selected.timezoneMode === 'local'
    return {
      offsetSeconds: selected.automaticOffsetSeconds,
      determined: true,
      syncWarning: selected.overlaps ? null : i18next.t('store.videoOutsideActivity', 'Video is outside the activity'),
      timezoneMode: ambiguous && !inferredLocalOnly ? selected.timezoneMode : null,
    }
  } catch (error) {
    if (!(error instanceof VideoSyncError)) throw error
    return {
      offsetSeconds: 0,
      determined: false,
      syncWarning: i18next.t(error.translationKey, error.message),
      timezoneMode: null,
    }
  }
}

/**
 * Resolves automatic synchronization for every clip against one external activity.
 * An explicit session mode applies to all clips. Without one, each ambiguous clip
 * infers its own interpretation; the session adopts one only when every ambiguous
 * clip settled on the same explicit interpretation (a sole local match stays automatic).
 * @param {Array<object>} videos Imported video entities.
 * @param {object} activitySummary External activity summary.
 * @param {'utc'|'local'|null} sessionMode Explicit Apply Timezone selection, or null to infer.
 * @returns {{placements: Array<{offsetSeconds: number, determined: boolean, syncWarning: string|null}>, timezoneMode: 'utc'|'local'|null}}
 *   Placements in input order and the resulting session mode.
 */
export function resolveVideosSync(videos, activitySummary, sessionMode) {
  const results = videos.map((video) => resolveVideoSync(video, activitySummary, sessionMode))
  const ambiguousModes = results.filter((_, index) => isAmbiguousTimeSource(videos[index].timeSource)).map(({ timezoneMode }) => timezoneMode)
  let timezoneMode = null
  if (sessionMode !== null) timezoneMode = ambiguousModes.length > 0 ? sessionMode : null
  else if (ambiguousModes.length > 0 && ambiguousModes.every((mode) => mode !== null && mode === ambiguousModes[0])) timezoneMode = ambiguousModes[0]
  return {
    placements: results.map(({ offsetSeconds, determined, syncWarning }) => ({ offsetSeconds, determined, syncWarning })),
    timezoneMode,
  }
}

function parseCreationTimeMs(video) {
  if (typeof video.creationTime !== 'string' || video.creationTime.trim() === '') return null
  if (video.timeSource === 'gps') {
    const parsed = Date.parse(video.creationTime)
    return Number.isFinite(parsed) ? parsed : null
  }
  return parseSyncTimestamp(video.creationTime, video.timeSource, null)
}

/**
 * Places clips without an external activity at their detected real time relative
 * to the earliest clip, keeping the gaps between recordings. Ambiguous camera
 * clock text is compared as wall-clock time.
 * @param {Array<object>} videos Imported video entities.
 * @returns {Array<{offsetSeconds: number, determined: boolean, syncWarning: string|null}>} Placements in input order.
 */
export function placeVideosByCreationTime(videos) {
  const startsMs = videos.map(parseCreationTimeMs)
  const knownStartsMs = startsMs.filter((startMs) => startMs !== null)
  const originMs = knownStartsMs.length > 0 ? Math.min(...knownStartsMs) : null
  return startsMs.map((startMs) =>
    startMs === null
      ? {
          offsetSeconds: 0,
          determined: false,
          syncWarning: i18next.t('store.couldNotDetermineVideoCreationTime', 'Could not determine video creation time'),
        }
      : { offsetSeconds: (startMs - originMs) / 1000, determined: true, syncWarning: null },
  )
}

/**
 * State of the automatic video-sync controls, derived from a video collection
 * (the store, or the draft of the import dialog).
 */

import { parseVideoFilenameCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'

/**
 * The shared offset shows where the earliest video starts, on that video's
 * frames; editing it shifts every video by the same amount.
 * @param {{importedVideos: Array<object>, videoEdits: object, videoSyncTimezoneMode: 'utc'|'local'|null, parsedActivity: object|null, activitySummary: object|null}} collection
 *   Video collection with the external activity.
 * @returns {object} Control state (see `VideoSyncControls`).
 */
export function deriveVideoSyncControls(collection) {
  const { importedVideos, videoEdits, videoSyncTimezoneMode, parsedActivity, activitySummary } = collection
  const orderedVideos = sortVideosByTimeline(joinVideoEdits(importedVideos, videoEdits))
  return {
    activitySummary,
    canResetCreationTime: importedVideos.some((video) => video.timeSource === 'filename'),
    filenameCreationTimeAvailable: importedVideos.some((video) => parseVideoFilenameCreationTime(video.path) !== null),
    sharedOffsetSeconds: orderedVideos[0]?.offsetSeconds ?? 0,
    sharedOffsetFps: orderedVideos[0]?.fps ?? null,
    timezone: parsedActivity?.metadata?.timezone ?? null,
    timezoneDisabled: importedVideos.length > 0 && importedVideos.every((video) => video.timeSource === 'filename'),
    videoSyncTimezoneMode,
    videoSyncWarning: orderedVideos.find((video) => video.syncWarning !== null)?.syncWarning ?? null,
  }
}

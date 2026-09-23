import { CornerUpLeft, CornerUpRight, MapPin, OctagonMinus } from 'lucide-react'
import { formatClockDuration } from '@/lib/time-format'
import { getInterpolatedActivityValue, getMetricSeries } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { formatStandardMetricDisplay } from '@/features/widget-preview/widgets/metric/format'
import { VIDEO_SYNC_LANDMARK_TYPES, VIDEO_SYNC_PREVIEW_SCREEN_GAP } from '../data/videoSyncConstants'

const SPEED_FORMAT = Object.freeze({ decimals: 1, display_unit: 'kmh', show_units: true })

export const VIDEO_SYNC_LANDMARK_PRESENTATION = {
  [VIDEO_SYNC_LANDMARK_TYPES.STOP]: {
    Icon: OctagonMinus,
    className: 'text-video-sync-stop',
    defaultLabel: 'Stop',
    labelKey: 'videoSync.stop',
    stripe: 'bg-video-sync-stop',
  },
  [VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN]: {
    Icon: CornerUpLeft,
    className: 'text-video-sync-turn',
    defaultLabel: 'Left Turn',
    labelKey: 'videoSync.leftTurn',
    stripe: 'bg-video-sync-turn',
  },
  [VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN]: {
    Icon: CornerUpRight,
    className: 'text-video-sync-turn',
    defaultLabel: 'Right Turn',
    labelKey: 'videoSync.rightTurn',
    stripe: 'bg-video-sync-turn',
  },
  [VIDEO_SYNC_LANDMARK_TYPES.LOCATION]: {
    Icon: MapPin,
    className: 'text-video-sync-location',
    defaultLabel: 'Location',
    labelKey: 'videoSync.location',
    stripe: 'bg-video-sync-location',
  },
}

/**
 * @param {{width: number, height: number}} sceneSize Video dimensions.
 * @param {number} displayScale Preview scale.
 * @returns {object} Screen and pair styles.
 */
export function buildVideoSyncScreenLayout(sceneSize, displayScale) {
  const width = sceneSize.width * displayScale
  const height = sceneSize.height * displayScale
  const gap = VIDEO_SYNC_PREVIEW_SCREEN_GAP * displayScale
  return {
    screenStyle: { width, height },
    pairStyle: { gridTemplateColumns: `repeat(2, ${width}px)`, width: width * 2 + gap, height, gap },
  }
}

/** @param {object|null} activity Parsed activity. @param {number} previewSecond Activity second. @returns {string|null} Speed readout. */
export function getVideoSyncPreviewSpeed(activity, previewSecond) {
  const speed = getMetricSeries(activity, 'speed')
  if (!Array.isArray(speed) || !speed.some((value) => value !== null && value !== undefined)) return null
  return formatStandardMetricDisplay('speed', getInterpolatedActivityValue(activity, 'speed', previewSecond), SPEED_FORMAT)
}

/**
 * @param {object[]} landmarks Canonical landmarks.
 * @param {object|null} detection Current detection.
 * @param {object} drafts Sensitivity drafts.
 * @returns {object} Drawer presentation data.
 */
export function buildVideoSyncDrawerPresentation(landmarks, detection, drafts) {
  const detectionCounts =
    detection === null
      ? { stops: 0, leftTurns: 0, rightTurns: 0, locations: 0 }
      : {
          stops: detection.stops.length,
          leftTurns: detection.turns.filter((event) => event.type === VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN).length,
          rightTurns: detection.turns.filter((event) => event.type === VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN).length,
          locations: detection.location === null ? 0 : 1,
        }

  return {
    detectionCounts,
    speedValueLabel: `${drafts.speedThresholdDraftKmh.toFixed(1)}km/h`,
    turnValueLabel: `${drafts.turnThresholdDraftDegrees.toFixed(1)}°`,
    landmarkCards: [...landmarks]
      .sort((left, right) => left.videoSecond - right.videoSecond || left.id.localeCompare(right.id))
      .map((landmark) => ({
        landmark,
        presentation: VIDEO_SYNC_LANDMARK_PRESENTATION[landmark.type],
        videoTimeLabel: formatClockDuration(landmark.videoSecond),
      })),
  }
}

/** @param {object[]} candidates Canonical candidates. @param {number} appliedOffset Current offset. @returns {object[]} Candidate card models. */
export function buildVideoSyncCandidateCards(candidates, appliedOffset) {
  return candidates.map((candidate) => ({
    candidate,
    formattedOffset: formatClockDuration(candidate.offset),
    isApplied: candidate.offset === appliedOffset,
    isLocationOnly: candidate.variant === 'locationOnly',
    scoreColor: candidate.matchScore === null ? null : `hsl(${candidate.matchScore * 1.2} 72% 48%)`,
  }))
}

/**
 * Resolves mark availability at the current activity playhead.
 * @param {object|null} markControls Workspace mark state.
 * @param {number} timelineSecond Current activity second.
 * @param {boolean} enabled Whether the sync workspace is active.
 * @returns {object|null} Resolved controls, or null outside sync mode.
 */
export function resolveVideoSyncMarkControls(markControls, timelineSecond, enabled) {
  if (!enabled || markControls === null) return null

  const videoSecond = timelineSecond - markControls.videoSyncOffsetSeconds
  const hasVideo = markControls.importedVideoDuration !== null
  const isInsideVideo = hasVideo && videoSecond >= 0 && videoSecond < markControls.importedVideoDuration
  const canMark = isInsideVideo && markControls.hasLandmarkCapacity
  const markDisabledReason = !hasVideo
    ? markControls.videoRequiredReason
    : !isInsideVideo
      ? markControls.playheadOutsideVideoReason
      : !markControls.hasLandmarkCapacity
        ? markControls.landmarkLimitReason
        : null

  return {
    canMark,
    canMarkLocation: canMark && !markControls.hasLocationLandmark,
    locationDisabledReason: markControls.hasLocationLandmark ? markControls.locationLimitReason : markDisabledReason,
    markDisabledReason,
    onMarkLeftTurn: markControls.onMarkLeftTurn,
    onMarkLocation: markControls.onMarkLocation,
    onMarkRightTurn: markControls.onMarkRightTurn,
    onMarkStop: markControls.onMarkStop,
    detection: markControls.detection,
    onDeleteCourseLocation: markControls.onDeleteCourseLocation,
    onSetCourseLocation: markControls.onSetCourseLocation,
  }
}

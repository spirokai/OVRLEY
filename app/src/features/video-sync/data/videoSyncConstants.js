import { CornerUpLeft, CornerUpRight, MapPin, OctagonMinus } from 'lucide-react'

export const VIDEO_SYNC_LANDMARK_TYPES = {
  STOP: 'stop',
  LEFT_TURN: 'leftTurn',
  RIGHT_TURN: 'rightTurn',
  LOCATION: 'location',
}

export const VIDEO_SYNC_MAX_LANDMARKS = 5
export const VIDEO_SYNC_MAX_LOCATION_LANDMARKS = 1

export const VIDEO_SYNC_DEFAULT_SPEED_THRESHOLD_KMH = 5
export const VIDEO_SYNC_SPEED_THRESHOLD_RANGE_KMH = { min: 1, max: 10 }
export const VIDEO_SYNC_KMH_TO_METERS_PER_SECOND = 1000 / 3600

export const VIDEO_SYNC_DEFAULT_TURN_THRESHOLD_DEGREES = 80
export const VIDEO_SYNC_TURN_THRESHOLD_RANGE_DEGREES = { min: 70, max: 180 }

export const VIDEO_SYNC_STOP_ENTRY_DWELL_SECONDS = 2
export const VIDEO_SYNC_STOP_EXIT_DWELL_SECONDS = 2
export const VIDEO_SYNC_STOP_EXIT_HYSTERESIS_KMH = 2
export const VIDEO_SYNC_TURN_MAXIMUM_DURATION_SECONDS = 10
export const VIDEO_SYNC_TURN_REVERSAL_TOLERANCE_DEGREES = 20
export const VIDEO_SYNC_HEADING_SMOOTHING_WINDOW_SECONDS = 1
export const VIDEO_SYNC_USER_TIMING_TOLERANCE_SECONDS = 2
export const VIDEO_SYNC_SIGNIFICANT_GAP_BASE_SECONDS = 3
export const VIDEO_SYNC_SIGNIFICANT_GAP_CADENCE_MULTIPLIER = 3
export const VIDEO_SYNC_CANDIDATE_MERGE_TOLERANCE_SECONDS = 10
export const VIDEO_SYNC_MAX_CANDIDATES = 5

// Graph display constants. These affect rendered geometry only; detector input
// and matching values remain unmodified.
export const VIDEO_SYNC_GRAPH_ROBUST_PERCENTILE = 0.99
export const VIDEO_SYNC_GRAPH_MIN_SCALE_VALUE = 1
export const VIDEO_SYNC_GRAPH_HEIGHT_PX = 64

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

export const MANUAL_VIDEO_SYNC_CANDIDATE_STATUSES = {
  IDLE: 'idle',
  CALCULATING: 'calculating',
  FRESH: 'fresh',
  STALE: 'stale',
  ERROR: 'error',
}

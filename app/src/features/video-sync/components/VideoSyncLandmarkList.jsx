import { CircleStop, CornerUpLeft, CornerUpRight, MapPin, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { SectionHeading } from '@/components/ui/section-heading'
import { formatClockDuration } from '@/lib/time-format'
import { VIDEO_SYNC_LANDMARK_TYPES } from '../data/videoSyncConstants'

const LANDMARK_PRESENTATION = {
  [VIDEO_SYNC_LANDMARK_TYPES.STOP]: {
    Icon: CircleStop,
    labelKey: 'videoSync.stop',
    defaultLabel: 'Stop',
    className: 'text-video-sync-stop',
    stripe: 'bg-video-sync-stop',
  },
  [VIDEO_SYNC_LANDMARK_TYPES.LEFT_TURN]: {
    Icon: CornerUpLeft,
    labelKey: 'videoSync.leftTurn',
    defaultLabel: 'Left Turn',
    className: 'text-video-sync-turn',
    stripe: 'bg-video-sync-turn',
  },
  [VIDEO_SYNC_LANDMARK_TYPES.RIGHT_TURN]: {
    Icon: CornerUpRight,
    labelKey: 'videoSync.rightTurn',
    defaultLabel: 'Right Turn',
    className: 'text-video-sync-turn',
    stripe: 'bg-video-sync-turn',
  },
  [VIDEO_SYNC_LANDMARK_TYPES.LOCATION]: {
    Icon: MapPin,
    labelKey: 'videoSync.location',
    defaultLabel: 'Location',
    className: 'text-video-sync-location',
    stripe: 'bg-video-sync-location',
  },
}

/**
 * Renders the sorted manual landmark cards.
 *
 * @param {object} props Landmark state and callbacks.
 * @param {object[]} props.landmarks Canonical video landmarks.
 * @param {() => void} props.onClear Clears all landmarks.
 * @param {(id: string) => void} props.onDelete Deletes one landmark.
 * @param {(landmark: object) => void} props.onScrub Scrubs to a landmark.
 * @returns {JSX.Element} Rendered landmark list.
 */
export function VideoSyncLandmarkList({ landmarks, onClear, onDelete, onScrub }) {
  const { t } = useTranslation()
  const sortedLandmarks = [...landmarks].sort((left, right) => left.videoSecond - right.videoSecond || left.id.localeCompare(right.id))

  return (
    <section className="space-y-3" aria-label={t('videoSync.landmarks', 'Landmarks')}>
      <SectionHeading
        icon={MapPin}
        title={t('videoSync.landmarks', 'Landmarks')}
        variant="drawer"
        trailing={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-[10px] text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
            disabled={landmarks.length === 0}
            onClick={onClear}
          >
            {t('videoSync.clearLandmarks', 'Clear')}
          </Button>
        }
      />

      {sortedLandmarks.length > 0 ? (
        <div className="space-y-1.5" role="list" aria-label={t('videoSync.videoLandmarks', 'Video landmarks')}>
          {sortedLandmarks.map((landmark) => {
            const presentation = LANDMARK_PRESENTATION[landmark.type]
            const Icon = presentation.Icon
            const videoTimeLabel = formatClockDuration(landmark.videoSecond)
            return (
              <div
                key={landmark.id}
                role="listitem"
                className="relative flex min-h-10 items-stretch overflow-hidden rounded-xs border border-border/70 bg-surface-elevated/70"
              >
                <div className={`w-1 shrink-0 ${presentation.stripe}`} aria-hidden="true" />
                <button
                  type="button"
                  className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
                  aria-label={t('videoSync.scrubLandmark', 'Go to {{type}} at {{time}}', {
                    type: t(presentation.labelKey, presentation.defaultLabel),
                    time: videoTimeLabel,
                  })}
                  onClick={() => onScrub(landmark)}
                >
                  <Icon className={`size-4 shrink-0 ${presentation.className}`} aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
                    {t(presentation.labelKey, presentation.defaultLabel)}
                  </span>
                  <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{videoTimeLabel}</span>
                </button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="mr-1 h-7 w-7 self-center text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
                  aria-label={t('videoSync.deleteLandmark', 'Delete {{type}} landmark', {
                    type: t(presentation.labelKey, presentation.defaultLabel),
                  })}
                  onClick={() => onDelete(landmark.id)}
                >
                  <Trash2 className="size-3.5" />
                </Button>
              </div>
            )
          })}
        </div>
      ) : (
        <p className="px-1 text-xs text-muted-foreground">{t('videoSync.noLandmarks', 'No landmarks marked yet.')}</p>
      )}
    </section>
  )
}

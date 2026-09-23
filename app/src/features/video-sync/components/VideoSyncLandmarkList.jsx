import { MapPin, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { SectionHeading } from '@/components/ui/section-heading'
import { VIDEO_SYNC_LANDMARK_TYPES } from '../data/videoSyncConstants'
import { VIDEO_SYNC_LANDMARK_PRESENTATION } from '../utils/videoSyncPresentation'

/**
 * Renders the sorted manual landmark cards.
 *
 * @param {object} props Landmark state and callbacks.
 * @param {object[]} props.landmarkCards Prepared landmark presentation.
 * @param {() => void} props.onClear Clears all landmarks.
 * @param {(id: string, type: string) => void} props.onChangeType Changes one landmark type.
 * @param {(id: string) => void} props.onDelete Deletes one landmark.
 * @returns {JSX.Element} Rendered landmark list.
 */
export function VideoSyncLandmarkList({ hasLocationLandmark, landmarkCards, onClear, onChangeType, onDelete }) {
  const { t } = useTranslation()

  return (
    <section className="space-y-3" aria-label={t('videoSync.landmarks', 'Landmarks')}>
      <SectionHeading
        icon={MapPin}
        title={t('videoSync.yourLandmarks', 'Video Landmarks')}
        variant="drawer"
        trailing={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-[10px] text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
            disabled={landmarkCards.length === 0}
            onClick={onClear}
          >
            {t('videoSync.clearLandmarks', 'Clear')}
          </Button>
        }
      />

      {landmarkCards.length > 0 ? (
        <div className="space-y-1.5" role="list" aria-label={t('videoSync.videoLandmarks', 'Video landmarks')}>
          {landmarkCards.map(({ landmark, presentation, videoTimeLabel }) => {
            return (
              <div
                key={landmark.id}
                role="listitem"
                className="relative flex min-h-10 items-stretch overflow-hidden rounded-xs border border-border/70 bg-surface-elevated/70"
              >
                <div className={`w-1 shrink-0 mr-1 ${presentation.stripe}`} aria-hidden="true" />
                <Select value={landmark.type} onValueChange={(type) => onChangeType(landmark.id, type)}>
                  <SelectTrigger
                    size="sm"
                    className={`mr-1 h-7 w-40 shrink self-center border-transparent bg-transparent px-1.5 text-[0.7rem] uppercase font-semibold shadow-none [&>svg:last-child]:text-current hover:border-transparent hover:bg-surface-accent-soft focus-visible:border-transparent focus-visible:ring-2 focus-visible:ring-primary/60 ${presentation.className}`}
                    aria-label={t('videoSync.changeLandmarkType', 'Change {{type}} landmark type', {
                      type: t(presentation.labelKey, presentation.defaultLabel),
                    })}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent align="end">
                    {Object.values(VIDEO_SYNC_LANDMARK_TYPES).map((type) => {
                      const option = VIDEO_SYNC_LANDMARK_PRESENTATION[type]
                      const OptionIcon = option.Icon
                      const isLocationTaken = type === VIDEO_SYNC_LANDMARK_TYPES.LOCATION && landmark.type !== type && hasLocationLandmark
                      return (
                        <SelectItem key={type} value={type} disabled={isLocationTaken}>
                          <OptionIcon className={`size-3.5 ${option.className}`} aria-hidden="true" />
                          {t(option.labelKey, option.defaultLabel)}
                        </SelectItem>
                      )
                    })}
                  </SelectContent>
                </Select>
                <span className="flex min-w-0 flex-1 items-center justify-end px-1 text-xs tabular-nums text-muted-foreground">{videoTimeLabel}</span>
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
        <p className="px-1 text-xs text-muted-foreground/50">{t('videoSync.noLandmarks', 'No landmarks marked yet.')}</p>
      )}
    </section>
  )
}

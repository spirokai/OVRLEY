import { CornerUpLeft, CornerUpRight, MapPin, OctagonMinus } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'

/**
 * Renders the four typed manual landmark controls over the video preview.
 *
 * @param {object} props Mark action state and callbacks.
 * @param {boolean} props.canMark Whether stop and turn marks can be created.
 * @param {boolean} props.canMarkLocation Whether a location mark can be created.
 * @param {string|null} props.markDisabledReason Explanation for disabled stop/turn actions.
 * @param {string|null} props.locationDisabledReason Explanation for disabled location action.
 * @param {() => void} props.onMarkStop Creates a stop landmark.
 * @param {() => void} props.onMarkLeftTurn Creates a left-turn landmark.
 * @param {() => void} props.onMarkRightTurn Creates a right-turn landmark.
 * @param {() => void} props.onMarkLocation Creates a location landmark.
 * @returns {JSX.Element} Rendered mark controls.
 */
export function VideoSyncMarkControls({
  canMark,
  canMarkLocation,
  locationDisabledReason,
  markDisabledReason,
  onMarkLeftTurn,
  onMarkLocation,
  onMarkRightTurn,
  onMarkStop,
}) {
  const { t } = useTranslation()
  const controls = [
    {
      Icon: CornerUpLeft,
      colorClassName:
        'border-video-sync-turn/70 text-video-sync-turn hover:bg-video-sync-turn/20 hover:text-video-sync-turn focus-visible:ring-video-sync-turn/50',
      disabled: !canMark,
      disabledReason: markDisabledReason,
      label: t('videoSync.markLeftTurn', 'Mark Left Turn'),
      onClick: onMarkLeftTurn,
    },
    {
      Icon: OctagonMinus,
      colorClassName:
        'border-video-sync-stop/70 text-video-sync-stop hover:bg-video-sync-stop/20 hover:text-video-sync-stop focus-visible:ring-video-sync-stop/50',
      disabled: !canMark,
      disabledReason: markDisabledReason,
      label: t('videoSync.markStop', 'Mark Stop'),
      onClick: onMarkStop,
    },
    {
      Icon: MapPin,
      colorClassName:
        'border-video-sync-location/70 text-video-sync-location hover:bg-video-sync-location/20 hover:text-video-sync-location focus-visible:ring-video-sync-location/50',
      disabled: !canMarkLocation,
      disabledReason: locationDisabledReason,
      label: t('videoSync.markLocation', 'Mark Location'),
      onClick: onMarkLocation,
    },
    {
      Icon: CornerUpRight,
      colorClassName:
        'border-video-sync-turn/70 text-video-sync-turn hover:bg-video-sync-turn/20 hover:text-video-sync-turn focus-visible:ring-video-sync-turn/50',
      disabled: !canMark,
      disabledReason: markDisabledReason,
      label: t('videoSync.markRightTurn', 'Mark Right Turn'),
      onClick: onMarkRightTurn,
    },
  ]

  return (
    <div
      data-testid="video-sync-mark-controls"
      className="pointer-events-auto absolute bottom-4 left-1/2 z-50 flex w-max max-w-full -translate-x-1/2 flex-row gap-1 "
      aria-label={t('videoSync.markControls', 'Video sync landmark controls')}
    >
      {controls.map(({ Icon, colorClassName, disabled, disabledReason, label, onClick }) => (
        <div key={label} className="flex flex-1 rounded-sm bg-surface shadow-md">
          <Button
            variant="ghost"
            size="sm"
            className={`h-7 uppercase w-auto justify-center border bg-surface px-2 text-[0.75rem] font-semibold ${colorClassName}`}
            disabled={disabled}
            title={disabledReason ?? label}
            aria-label={label}
            onClick={onClick}
          >
            <Icon className="size-3.5 shrink-0" />
            <span>{label}</span>
          </Button>
        </div>
      ))}
    </div>
  )
}

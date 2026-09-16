import { CircleStop, CornerUpLeft, CornerUpRight, MapPin } from 'lucide-react'
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
      Icon: CircleStop,
      colorClassName:
        'border-video-sync-stop/70 text-video-sync-stop hover:bg-surface-elevated hover:text-video-sync-stop focus-visible:ring-video-sync-stop/50',
      disabled: !canMark,
      disabledReason: markDisabledReason,
      label: t('videoSync.markStop', 'Mark Stop'),
      onClick: onMarkStop,
    },
    {
      Icon: CornerUpLeft,
      colorClassName:
        'border-video-sync-turn/70 text-video-sync-turn hover:bg-surface-elevated hover:text-video-sync-turn focus-visible:ring-video-sync-turn/50',
      disabled: !canMark,
      disabledReason: markDisabledReason,
      label: t('videoSync.markLeftTurn', 'Mark Left Turn'),
      onClick: onMarkLeftTurn,
    },
    {
      Icon: CornerUpRight,
      colorClassName:
        'border-video-sync-turn/70 text-video-sync-turn hover:bg-surface-elevated hover:text-video-sync-turn focus-visible:ring-video-sync-turn/50',
      disabled: !canMark,
      disabledReason: markDisabledReason,
      label: t('videoSync.markRightTurn', 'Mark Right Turn'),
      onClick: onMarkRightTurn,
    },
    {
      Icon: MapPin,
      colorClassName:
        'border-video-sync-location/70 text-video-sync-location hover:bg-surface-elevated hover:text-video-sync-location focus-visible:ring-video-sync-location/50',
      disabled: !canMarkLocation,
      disabledReason: locationDisabledReason,
      label: t('videoSync.markLocation', 'Mark Location'),
      onClick: onMarkLocation,
    },
  ]

  return (
    <div
      data-testid="video-sync-mark-controls"
      className="pointer-events-auto absolute bottom-4 left-4 z-50 flex w-44 flex-col gap-1 rounded-xs border border-border/70 bg-card p-1 shadow-lg"
      aria-label={t('videoSync.markControls', 'Video sync landmark controls')}
    >
      {controls.map(({ Icon, colorClassName, disabled, disabledReason, label, onClick }) => (
        <Button
          key={label}
          type="button"
          variant="ghost"
          size="sm"
          className={`h-7 w-full min-w-0 justify-start border bg-surface px-2 text-[10px] font-semibold ${colorClassName}`}
          disabled={disabled}
          title={disabledReason ?? label}
          aria-label={label}
          onClick={onClick}
        >
          <Icon className="size-3.5 shrink-0" />
          <span className="min-w-0 truncate">{label}</span>
        </Button>
      ))}
    </div>
  )
}

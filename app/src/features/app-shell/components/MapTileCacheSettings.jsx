import { Button } from '@/components/ui/button'
import { HelpTooltip } from '@/components/ui/help-tooltip'
import { Progress } from '@/components/ui/progress'
import { SliderField } from '@/features/widget-editor/components/widgetFormControls'
import { formatCacheBytes } from '../utils/formatCacheBytes'
import { useTranslation } from 'react-i18next'

/**
 * Renders the map tile cache usage, its size limit slider, and the clear action.
 *
 * @param {object} props - Component props.
 * @param {import('@/api/backend').MapTileCacheUsage} props.usage - Current cache usage and limit bounds.
 * @param {number} props.limitBytes - Slider limit, a draft while dragging.
 * @param {boolean} props.pending - Whether a cache command is running; disables clearing.
 * @param {(limitBytes: number) => void} props.onLimitDraft - Callback invoked while the slider moves.
 * @param {(limitBytes: number) => void} props.onLimitCommit - Callback invoked when the slider is released.
 * @param {() => void} props.onClear - Callback invoked to clear the cache.
 * @returns {JSX.Element} Rendered cache settings.
 */
export default function MapTileCacheSettings({ usage, limitBytes, pending, onLimitDraft, onLimitCommit, onClear }) {
  const { t } = useTranslation()

  return (
    <section className="space-y-4 border-t border-border/60 pt-4">
      <div className="flex items-center justify-between gap-6">
        <div className="min-w-0 space-y-2">
          <div className="text-sm text-foreground">{t('app-shell.mapTileCache', 'Map tile cache')}</div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span>
              {t('app-shell.mapTileCacheUsage', '{{used}} of {{limit}} used', {
                used: formatCacheBytes(usage.usedBytes),
                limit: formatCacheBytes(usage.limitBytes),
              })}
            </span>
            <HelpTooltip portal content={t('app-shell.mapTileCacheHelp')} />
          </div>
        </div>
        <Button type="button" variant="outline" size="sm" className="shrink-0" disabled={pending || usage.usedBytes === 0} onClick={onClear}>
          {t('app-shell.clearMapTileCache', 'Clear cache')}
        </Button>
      </div>
      {/* A neutral storage meter, so it does not read as a slider track without its thumb. */}
      <Progress
        value={(usage.usedBytes / usage.limitBytes) * 100}
        className="h-1 rounded-xs bg-surface-strong [&>[data-slot=progress-indicator]]:bg-muted-foreground"
      />
      <SliderField
        label={t('app-shell.mapTileCacheLimit', 'Cache size')}
        min={usage.minLimitBytes}
        max={usage.maxLimitBytes}
        step={usage.limitStepBytes}
        value={limitBytes}
        valueDisplay={formatCacheBytes(limitBytes)}
        onSliderChange={onLimitDraft}
        onSliderCommit={onLimitCommit}
      />
    </section>
  )
}

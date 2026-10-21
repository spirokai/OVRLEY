/**
 * Renders shared custom export range controls.
 */

import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { SecondsInput } from '@/components/ui/seconds-input'
import { Switch } from '@/components/ui/switch'
import { useExportRangeSettings } from '../hooks/useRenderVideoDerivedState'
import { useTranslation } from 'react-i18next'

/**
 * Renders the export range settings component.
 *
 * @param {object} props - Component props.
 * @param {*} props.range - Export range state object.
 * @param {number} props.fps - Render frame rate the boundaries snap to.
 * @param {*} props.onExportRangeChange - Callback invoked when range changes.
 * @param {boolean} [props.showUseVideoRangeAction=false] - Whether to show the imported-video range action.
 * @param {function} [props.onUseVideoRange] - Callback invoked when the imported-video range action is selected.
 * @returns {JSX.Element} Rendered component output.
 */
export default function ExportRangeSettings({ range, fps, onExportRangeChange, showUseVideoRangeAction = false, onUseVideoRange }) {
  const { t } = useTranslation()
  const ctx = useExportRangeSettings({ range, fps, onExportRangeChange })

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <Label className="text-xs font-medium">{t('render-video.customExportRange', 'Custom Export Range')}</Label>
        </div>
        <Switch checked={ctx.isCustom} onCheckedChange={ctx.handleCustomChange} />
      </div>

      {ctx.isCustom ? (
        <div className={`grid gap-4 ${showUseVideoRangeAction ? 'grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]' : 'grid-cols-2'}`}>
          <div className="space-y-1.5">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('render-video.from', 'From')}</Label>
            <SecondsInput
              value={ctx.fromSecond}
              fps={ctx.rangeFps}
              signed
              onCommit={ctx.handleFromCommit}
              className="h-9 text-xs"
              aria-label={t('render-video.from', 'From')}
            />
          </div>

          <div className="space-y-1.5">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('render-video.to', 'To')}</Label>
            <SecondsInput
              value={ctx.toSecond}
              fps={ctx.rangeFps}
              signed
              onCommit={ctx.handleToCommit}
              className="h-9 text-xs"
              aria-label={t('render-video.to', 'To')}
            />
          </div>

          {showUseVideoRangeAction ? (
            <div className="flex items-end">
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="h-9 border-border/80 bg-surface-elevated px-2 text-[10px] font-semibold text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
                onClick={onUseVideoRange}
              >
                {t('render-video.useVideoRange', 'Use Video Range')}
              </Button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

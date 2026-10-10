/**
 * Renders shared custom export range controls.
 */

import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Switch } from '@/components/ui/switch'
import { useExportRangeSettings } from '../hooks/useRenderVideoDerivedState'
import { useTranslation } from 'react-i18next'

/**
 * Renders the export range settings component.
 *
 * @param {object} props - Component props.
 * @param {*} props.range - Export range state object.
 * @param {*} props.onExportRangeChange - Callback invoked when range changes.
 * @param {boolean} [props.showUseVideoRangeAction=false] - Whether to show the imported-video range action.
 * @param {function} [props.onUseVideoRange] - Callback invoked when the imported-video range action is selected.
 * @returns {JSX.Element} Rendered component output.
 */
export default function ExportRangeSettings({ range, onExportRangeChange, showUseVideoRangeAction = false, onUseVideoRange }) {
  const { t } = useTranslation()
  const ctx = useExportRangeSettings({ range, onExportRangeChange })

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
            <BlurInput
              value={ctx.fromTime}
              onKeyDown={ctx.preventDecimalInput}
              onChange={ctx.handleFromChange}
              className="h-9 text-xs font-mono"
              placeholder={t('render-video.000000Or800', '00:00:00 or 800')}
            />
          </div>

          <div className="space-y-1.5">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('render-video.to', 'To')}</Label>
            <BlurInput
              value={ctx.toTime}
              onKeyDown={ctx.preventDecimalInput}
              onChange={ctx.handleToChange}
              className="h-9 text-xs font-mono"
              placeholder={t('render-video.000000Or900', '00:00:00 or 900')}
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

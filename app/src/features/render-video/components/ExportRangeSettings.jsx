/**
 * Renders shared custom export range controls.
 */

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
 * @returns {JSX.Element} Rendered component output.
 */
export default function ExportRangeSettings({ range, fps, onExportRangeChange }) {
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
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <Label className="text-[0.741rem] text-muted-foreground uppercase font-bold">{t('render-video.from', 'From')}</Label>
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
            <Label className="text-[0.741rem] text-muted-foreground uppercase font-bold">{t('render-video.to', 'To')}</Label>
            <SecondsInput
              value={ctx.toSecond}
              fps={ctx.rangeFps}
              signed
              onCommit={ctx.handleToCommit}
              className="h-9 text-xs"
              aria-label={t('render-video.to', 'To')}
            />
          </div>
        </div>
      ) : null}
    </div>
  )
}

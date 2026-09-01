/**
 * Renders shared custom export range controls.
 */

import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Switch } from '@/components/ui/switch'
import useStore from '@/store/useStore'
import {
  formatExportRangeTime,
  getActivityDurationSeconds,
  getCustomExportRangeDefault,
  setExportRangeBoundaryFromTimeInput,
} from '@/features/overlay-editor/utils/exportRange'
import { useTranslation } from 'react-i18next'

function sanitizeTimeInput(value) {
  const input = String(value).trim()
  const sign = input.startsWith('-') ? '-' : ''
  const sanitized = input
    .split(':')
    .map((part) => part.split(/[.,]/)[0].replace(/\D/g, ''))
    .join(':')
  return `${sign}${sanitized}`
}

function preventDecimalInput(event) {
  if (event.key === '.' || event.key === ',') {
    event.preventDefault()
  }
}

/**
 * Renders the export range settings component.
 *
 * @param {object} props - Component props.
 * @param {*} props.exportRange - Export range state object.
 * @param {*} props.onExportRangeChange - Callback invoked when range changes.
 * @param {boolean} [props.showUseVideoRangeAction=false] - Whether to show the imported-video range action.
 * @param {function} [props.onUseVideoRange] - Callback invoked when the imported-video range action is selected.
 * @returns {JSX.Element} Rendered component output.
 */
export default function ExportRangeSettings({ exportRange, onExportRangeChange, showUseVideoRangeAction = false, onUseVideoRange }) {
  const { t } = useTranslation()
  const parsedActivity = useStore((state) => state.parsedActivity)
  const activityEndSecond = getActivityDurationSeconds(parsedActivity)

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <Label className="text-xs font-medium">{t('render-video.customExportRange', 'Custom Export Range')}</Label>
        </div>
        <Switch
          checked={exportRange.type === 'custom'}
          onCheckedChange={(checked) =>
            onExportRangeChange(
              checked
                ? getCustomExportRangeDefault(exportRange, activityEndSecond)
                : {
                    ...exportRange,
                    type: 'all',
                  },
            )
          }
        />
      </div>

      {exportRange.type === 'custom' ? (
        <div className={`grid gap-4 ${showUseVideoRangeAction ? 'grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]' : 'grid-cols-2'}`}>
          <div className="space-y-1.5">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('render-video.from', 'From')}</Label>
            <BlurInput
              value={formatExportRangeTime(exportRange.from)}
              onKeyDown={preventDecimalInput}
              onChange={(event) =>
                onExportRangeChange(setExportRangeBoundaryFromTimeInput(exportRange, 'from', sanitizeTimeInput(event.target.value)))
              }
              className="h-9 text-xs font-mono"
              placeholder={t('render-video.000000Or800', '00:00:00 or 800')}
            />
          </div>

          <div className="space-y-1.5">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('render-video.to', 'To')}</Label>
            <BlurInput
              value={formatExportRangeTime(exportRange.to)}
              onKeyDown={preventDecimalInput}
              onChange={(event) => onExportRangeChange(setExportRangeBoundaryFromTimeInput(exportRange, 'to', sanitizeTimeInput(event.target.value)))}
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

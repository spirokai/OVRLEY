/**
 * Renders shared custom export range controls.
 */

import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Switch } from '@/components/ui/switch'

function sanitizeTimeInput(value) {
  return String(value)
    .split(':')
    .map((part) => part.split(/[.,]/)[0].replace(/\D/g, ''))
    .join(':')
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
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <Label className="text-xs font-medium">Custom Export Range</Label>
        </div>
        <Switch
          checked={exportRange.type === 'custom'}
          onCheckedChange={(checked) =>
            onExportRangeChange({
              ...exportRange,
              type: checked ? 'custom' : 'all',
            })
          }
        />
      </div>

      {exportRange.type === 'custom' ? (
        <div className={`grid gap-4 ${showUseVideoRangeAction ? 'grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]' : 'grid-cols-2'}`}>
          <div className="space-y-1.5">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">From</Label>
            <BlurInput
              value={exportRange.fromTime}
              onKeyDown={preventDecimalInput}
              onChange={(event) =>
                onExportRangeChange({
                  ...exportRange,
                  fromTime: sanitizeTimeInput(event.target.value),
                })
              }
              className="h-9 text-xs font-mono"
              placeholder="00:00:00 or 800"
            />
          </div>

          <div className="space-y-1.5">
            <Label className="text-[10px] text-muted-foreground uppercase font-bold">To</Label>
            <BlurInput
              value={exportRange.toTime}
              onKeyDown={preventDecimalInput}
              onChange={(event) =>
                onExportRangeChange({
                  ...exportRange,
                  toTime: sanitizeTimeInput(event.target.value),
                })
              }
              className="h-9 text-xs font-mono"
              placeholder="00:00:00 or 900"
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
                Use Video Range
              </Button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

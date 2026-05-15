/**
 * Renders shared custom export range controls.
 */

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
 * @returns {JSX.Element} Rendered component output.
 */
export default function ExportRangeSettings({ exportRange, onExportRangeChange }) {
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
        <div className="grid grid-cols-2 gap-4">
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
        </div>
      ) : null}
    </div>
  )
}

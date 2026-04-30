/**
 * Renders shared custom export range controls.
 */

import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Switch } from '@/components/ui/switch'

/**
 * Renders the export range settings component.
 *
 * @param {object} props - Component props.
 * @param {*} props.exportRange - Export range state object.
 * @param {*} props.onExportRangeChange - Callback invoked when range changes.
 * @returns {JSX.Element} Rendered component output.
 */
export default function ExportRangeSettings({
  exportRange,
  onExportRangeChange,
}) {
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
            <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
              From
            </Label>
            <BlurInput
              value={exportRange.fromTime}
              onChange={(event) =>
                onExportRangeChange({
                  ...exportRange,
                  fromTime: event.target.value,
                })
              }
              className="h-9 text-xs font-mono"
              placeholder="00:00:00 or 800"
            />
          </div>

          <div className="space-y-1.5">
            <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
              To
            </Label>
            <BlurInput
              value={exportRange.toTime}
              onChange={(event) =>
                onExportRangeChange({
                  ...exportRange,
                  toTime: event.target.value,
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

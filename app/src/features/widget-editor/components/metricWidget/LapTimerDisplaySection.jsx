import { FontSection } from '../widgetEditorSections'
import { SelectField, TextField, ToggleField } from '../widgetFormControls'
import { LAP_TIMER_MODES } from '@/lib/widget/standard-widgets'

const LAP_TIMER_LABELS = Object.fromEntries(LAP_TIMER_MODES.map((mode) => [mode.value, mode.label]))

/**
 * Renders the Current Lap and Best Lap configuration controls.
 *
 * @param {object} props
 * @param {object} props.widget - Lap timer widget configuration.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetSize - Applies live size changes.
 * @param {Function} props.commitWidgetSize - Commits live size changes.
 * @returns {JSX.Element} Lap timer controls.
 */
export default function LapTimerDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  return (
    <>
      <SelectField
        label="Readout"
        value={widget.data.lap_timer_mode}
        onValueChange={(mode) => updateWidgetData(widget.id, { lap_timer_mode: mode, label: LAP_TIMER_LABELS[mode] })}
        options={LAP_TIMER_MODES}
      />
      <FontSection widget={widget} updateWidgetData={updateWidgetData} updateWidgetSize={updateWidgetSize} commitWidgetSize={commitWidgetSize} />
      <div className="flex items-center justify-between py-2">
        <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Show Label</span>
        <ToggleField checked={widget.data.show_label} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_label: checked })} />
      </div>
      <TextField label="Label" value={widget.data.label} onChange={(label) => updateWidgetData(widget.id, { label })} />
    </>
  )
}

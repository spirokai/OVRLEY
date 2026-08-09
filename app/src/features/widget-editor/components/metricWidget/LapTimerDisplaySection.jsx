import { FontSection } from '../widgetEditorSections'
import { ColorField, SelectField, TextField, ToggleField } from '../widgetFormControls'
import { LAP_TIMER_MODES } from '@/lib/widget/standard-widgets'

const LAP_TIMER_MODES_BY_VALUE = Object.fromEntries(LAP_TIMER_MODES.map((mode) => [mode.value, mode]))

/**
 * Renders lap timer configuration controls.
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
        onValueChange={(mode) => {
          const selectedMode = LAP_TIMER_MODES_BY_VALUE[mode]
          if (!selectedMode) throw new Error(`Unsupported lap timer mode: ${mode}`)
          updateWidgetData(widget.id, { lap_timer_mode: mode, label: selectedMode.label, font_size: selectedMode.font_size })
        }}
        options={LAP_TIMER_MODES}
      />
      <FontSection widget={widget} updateWidgetData={updateWidgetData} updateWidgetSize={updateWidgetSize} commitWidgetSize={commitWidgetSize} />
      {widget.data.lap_timer_mode !== 'lap_log' ? (
        <>
          <div className="flex items-center justify-between py-2">
            <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Show Label</span>
            <ToggleField checked={widget.data.show_label} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_label: checked })} />
          </div>
          <TextField label="Label" value={widget.data.label} onChange={(label) => updateWidgetData(widget.id, { label })} />
        </>
      ) : null}
      {widget.data.lap_timer_mode === 'delta' || widget.data.lap_timer_mode === 'lap_log' ? (
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label="Positive Delta Color"
            value={widget.data.positive_delta_color}
            onChange={(positive_delta_color) => updateWidgetData(widget.id, { positive_delta_color })}
          />
          <ColorField
            label="Negative Delta Color"
            value={widget.data.negative_delta_color}
            onChange={(negative_delta_color) => updateWidgetData(widget.id, { negative_delta_color })}
          />
        </div>
      ) : null}
    </>
  )
}

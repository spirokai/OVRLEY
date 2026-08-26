import { Type } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import { SectionHeading } from '@/components/ui/section-heading'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { FontSection } from '../widgetEditorSections'
import { ColorField, SelectField, SizeSlider, TextField, ToggleField } from '../widgetFormControls'
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
  const availableFonts = useAvailableFonts()

  return (
    <>
      <SelectField
        label="Readout"
        value={widget.data.lap_timer_mode}
        onValueChange={(mode) => {
          const selectedMode = LAP_TIMER_MODES_BY_VALUE[mode]
          if (!selectedMode) throw new Error(`Unsupported lap timer mode: ${mode}`)
          updateWidgetData(widget.id, {
            lap_timer_mode: mode,
            label: selectedMode.label,
            font_size: selectedMode.font_size,
            label_font_size: selectedMode.label_font_size,
          })
        }}
        options={LAP_TIMER_MODES}
      />
      <FontSection widget={widget} updateWidgetData={updateWidgetData} updateWidgetSize={updateWidgetSize} commitWidgetSize={commitWidgetSize} />
      <div className="space-y-4">
        <div className="flex w-full items-center gap-3">
          <SectionHeading icon={Type} title="Label" />
          {widget.data.lap_timer_mode !== 'lap_log' ? (
            <div className="shrink-0 pt-1">
              <ToggleField checked={widget.data.show_label} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_label: checked })} />
            </div>
          ) : null}
        </div>
        {widget.data.lap_timer_mode !== 'lap_log' ? (
          <TextField label="Label" value={widget.data.label} onChange={(label) => updateWidgetData(widget.id, { label })} />
        ) : null}
        <SizeSlider
          label="Label Font Size"
          value={widget.data.label_font_size}
          min={6}
          max={100}
          step={0.5}
          valueDisplay={`${widget.data.label_font_size}px`}
          onChange={(label_font_size) => updateWidgetSize(widget.id, { label_font_size })}
          onCommit={() => commitWidgetSize(widget.id)}
        />
        <div className="grid grid-cols-2 items-end gap-4">
          <FontSelectField
            label="Label Font"
            value={widget.data.label_font}
            onValueChange={(label_font) => updateWidgetData(widget.id, { label_font })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <ColorField label="Label Color" value={widget.data.label_color} onChange={(label_color) => updateWidgetData(widget.id, { label_color })} />
        </div>
      </div>
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

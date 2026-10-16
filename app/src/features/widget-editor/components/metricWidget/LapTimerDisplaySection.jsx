import { DISPLAY_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { buildDisplayModeUpdate } from '@/lib/widget/widget-resolver'
import { Type } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import { SectionHeading } from '@/components/ui/section-heading'
import { useAvailableFonts } from '@/hooks/useFonts'
import { FontSection } from '../widgetEditorSections'
import { ColorField, SelectField, SizeSlider, TextField, ToggleField } from '../widgetFormControls'
import { useTranslation } from 'react-i18next'

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
  const { t } = useTranslation()
  const availableFonts = useAvailableFonts()

  return (
    <>
      <SelectField
        label={t('widget-editor.readout', 'Readout')}
        value={widget.data.lap_timer_mode}
        onValueChange={(mode) => updateWidgetData(widget.id, buildDisplayModeUpdate(widget.data.display_type, mode))}
        options={DISPLAY_TYPE_DEFINITIONS.lap_timer.modes.map(({ value, label, labelKey }) => ({ value, label: t(labelKey, label) }))}
      />
      <FontSection widget={widget} updateWidgetData={updateWidgetData} updateWidgetSize={updateWidgetSize} commitWidgetSize={commitWidgetSize} />
      <div className="space-y-4">
        <div className="flex w-full items-center gap-3">
          <SectionHeading icon={Type} title={t('widget-editor.label', 'Label')} />
          {widget.data.lap_timer_mode !== 'lap_log' ? (
            <div className="shrink-0 pt-1">
              <ToggleField checked={widget.data.show_label} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_label: checked })} />
            </div>
          ) : null}
        </div>
        {widget.data.lap_timer_mode !== 'lap_log' ? (
          <TextField
            label={t('widget-editor.label', 'Label')}
            value={widget.data.label}
            onChange={(label) => updateWidgetData(widget.id, { label })}
          />
        ) : null}
        <SizeSlider
          label={t('widget-editor.labelFontSize', 'Label Font Size')}
          value={widget.data.label_font_size}
          min={6}
          max={100}
          step={0.5}
          valueDisplay={`${widget.data.label_font_size}`}
          suffix="px"
          onChange={(label_font_size) => updateWidgetSize(widget.id, { label_font_size })}
          onCommit={() => commitWidgetSize(widget.id)}
        />
        <div className="grid grid-cols-2 items-end gap-4">
          <FontSelectField
            label={t('widget-editor.labelFont', 'Label Font')}
            value={widget.data.label_font}
            onValueChange={(label_font) => updateWidgetData(widget.id, { label_font })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <ColorField
            label={t('widget-editor.labelColor', 'Label Color')}
            value={widget.data.label_color}
            onChange={(label_color) => updateWidgetData(widget.id, { label_color })}
          />
        </div>
      </div>
      {widget.data.lap_timer_mode === 'delta' || widget.data.lap_timer_mode === 'lap_log' ? (
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.positiveDeltaColor', 'Positive Delta Color')}
            value={widget.data.positive_delta_color}
            onChange={(positive_delta_color) => updateWidgetData(widget.id, { positive_delta_color })}
          />
          <ColorField
            label={t('widget-editor.negativeDeltaColor', 'Negative Delta Color')}
            value={widget.data.negative_delta_color}
            onChange={(negative_delta_color) => updateWidgetData(widget.id, { negative_delta_color })}
          />
        </div>
      ) : null}
    </>
  )
}

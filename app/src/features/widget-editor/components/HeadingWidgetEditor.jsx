/**
 * Heading widget editor — provides controls for both text and heading_tape
 * display modes, selected via a display_type dropdown.
 *
 * When `display_type` is `text`, standard metric controls (font, icon, units)
 * are shown, matching MetricWidgetEditor.
 *
 * When `display_type` is `heading_tape`, heading-specific controls for tape
 * scale, ticks, labels, and indicator are shown. Display-specific settings
 * are stored in `display_variants.heading_tape`.
 *
 * Data is guaranteed to be complete by template normalization — no defensive
 * fallback values are needed. The variant is seeded from defaults on creation,
 * on display-type switch, and during template load.
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {Function} props.updateWidgetData - Updates widget data immutably.
 * @param {Function} props.setNumericField - Sets a numeric field on the widget.
 * @returns {JSX.Element} Rendered editor controls.
 */

import { Compass, Ruler, Type, Target } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { getFontFamilyName } from '@/lib/fonts'
import { ColorField, SliderField, ToggleField, SelectField } from './widgetFormControls'
import { SectionHeading } from './widgetEditorSections'
import { getDisplayTypeOptions } from '@/lib/standard-metrics'
import { isTextDisplayType } from '@/lib/display-type-behavior'
import { initDisplayVariant, buildFrameGeometryUpdate } from '@/lib/metric-widget-resolver'
import MetricWidgetEditor from './MetricWidgetEditor'

const ALIGNMENT_OPTIONS = [
  { value: 'below', label: 'Below' },
  { value: 'centered', label: 'Centered' },
]

const INDICATOR_STYLE_OPTIONS = [
  { value: 'chevron', label: 'Chevron' },
  { value: 'highlight_bar', label: 'Highlight Bar' },
]

const INDICATOR_PLACEMENT_OPTIONS = [
  { value: 'top', label: 'Top' },
  { value: 'bottom', label: 'Bottom' },
  { value: 'both', label: 'Both' },
]

export default function HeadingWidgetEditor({ widget, updateWidgetData, setNumericField }) {
  const data = widget.data ?? {}
  const displayType = data.display_type
  const tapeData = data.display_variants?.heading_tape ?? {}
  const availableFonts = useAvailableFonts()
  const showMajorTicks = tapeData.show_major_ticks
  const showMinorTicks = tapeData.show_minor_ticks
  const showMinorLabels = tapeData.show_minor_labels
  const showMajorLabels = tapeData.show_major_labels
  const isChevronIndicator = tapeData.indicator_style === 'chevron'

  const handleDisplayTypeChange = (value) => {
    const nextData = initDisplayVariant(data, value)
    updateWidgetData(widget.id, { display_type: value, display_variants: nextData.display_variants })
  }

  const updateTapeData = (updates) => {
    const nextVariant = { ...tapeData, ...updates }
    const geometryKeys = Object.keys(updates).filter((k) => k === 'width' || k === 'height' || k === 'rotation')
    const geometryPatch = geometryKeys.length > 0 ? Object.fromEntries(geometryKeys.map((k) => [k, updates[k]])) : null

    const patch = geometryPatch ? buildFrameGeometryUpdate(data, geometryPatch) : {}
    patch.display_variants = {
      ...(patch.display_variants || data.display_variants),
      heading_tape: nextVariant,
    }
    updateWidgetData(widget.id, patch)
  }

  return (
    <>
      {/* Display Type */}
      <div className="space-y-4">
        <SectionHeading icon={Compass} title="Display" />
        <SelectField label="Display Type" value={displayType} onValueChange={handleDisplayTypeChange} options={getDisplayTypeOptions('heading')} />
      </div>

      {isTextDisplayType(displayType) ? (
        <MetricWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} />
      ) : (
        <>
          {/* Tape Scale */}
          <div className="space-y-4">
            <SectionHeading icon={Compass} title="Tape Scale" />
            <SliderField
              label="Pixels per Degree"
              value={tapeData.pixels_per_degree}
              min={1}
              max={20}
              step={0.5}
              valueDisplay={`${tapeData.pixels_per_degree}px`}
              onSliderChange={(value) => updateTapeData({ pixels_per_degree: value })}
            />
          </div>

          {/* Ticks */}
          <div className="space-y-4">
            <SectionHeading icon={Ruler} title="Ticks" />
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-4">
                <div className="flex items-center justify-between gap-2 px-1">
                  <span className="text-[9px] text-muted-foreground uppercase font-bold">Major Ticks</span>
                  <ToggleField checked={showMajorTicks} onCheckedChange={(checked) => updateTapeData({ show_major_ticks: checked })} />
                </div>
                <SliderField
                  label="Major Length"
                  value={tapeData.major_tick_length_pct}
                  min={5}
                  max={100}
                  step={1}
                  disabled={!showMajorTicks}
                  valueDisplay={`${tapeData.major_tick_length_pct}%`}
                  onSliderChange={(value) => updateTapeData({ major_tick_length_pct: value })}
                />
                <SliderField
                  label="Major Thickness"
                  value={tapeData.major_tick_thickness}
                  min={0.5}
                  max={8}
                  step={0.5}
                  disabled={!showMajorTicks}
                  valueDisplay={`${tapeData.major_tick_thickness}px`}
                  onSliderChange={(value) => updateTapeData({ major_tick_thickness: value })}
                />
              </div>
              <div className="space-y-4">
                <div className="flex items-center justify-between gap-2 px-1">
                  <span className="text-[9px] text-muted-foreground uppercase font-bold">Minor Ticks</span>
                  <ToggleField checked={showMinorTicks} onCheckedChange={(checked) => updateTapeData({ show_minor_ticks: checked })} />
                </div>
                <SliderField
                  label="Minor Length"
                  value={tapeData.minor_tick_length_pct}
                  min={5}
                  max={100}
                  step={1}
                  disabled={!showMinorTicks}
                  valueDisplay={`${tapeData.minor_tick_length_pct}%`}
                  onSliderChange={(value) => updateTapeData({ minor_tick_length_pct: value })}
                />
                <SliderField
                  label="Minor Thickness"
                  value={tapeData.minor_tick_thickness}
                  min={0.5}
                  max={8}
                  step={0.5}
                  disabled={!showMinorTicks}
                  valueDisplay={`${tapeData.minor_tick_thickness}px`}
                  onSliderChange={(value) => updateTapeData({ minor_tick_thickness: value })}
                />
              </div>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <ColorField label="Tick Color" value={tapeData.tick_color} onChange={(value) => updateTapeData({ tick_color: value })} />
              <ColorField
                label="Cardinal Color"
                value={tapeData.cardinal_tick_color}
                onChange={(value) => updateTapeData({ cardinal_tick_color: value })}
              />
            </div>
            <SelectField
              label="Alignment"
              value={tapeData.tick_alignment}
              onValueChange={(value) => updateTapeData({ tick_alignment: value })}
              options={ALIGNMENT_OPTIONS}
            />
          </div>

          {/* Labels */}
          <div className="space-y-4">
            <SectionHeading icon={Type} title="Labels" />
            <FontSelectField
              label="Label Font"
              value={tapeData.label_font || tapeData.label_font_family}
              onValueChange={(value) => updateTapeData({ label_font: value, label_font_family: getFontFamilyName(value) })}
              recommendedFonts={availableFonts.recommendedFonts}
              systemFonts={availableFonts.systemFonts}
              triggerClassName="h-9 border-border/70 bg-surface text-xs"
              labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
            />
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">Minor Labels</span>
              <ToggleField checked={showMinorLabels} onCheckedChange={(checked) => updateTapeData({ show_minor_labels: checked })} />
            </div>
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">Major Labels</span>
              <ToggleField checked={showMajorLabels} onCheckedChange={(checked) => updateTapeData({ show_major_labels: checked })} />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <ColorField label="Label Color" value={tapeData.label_color} onChange={(value) => updateTapeData({ label_color: value })} />
              <ColorField
                label="Cardinal Color"
                value={tapeData.cardinal_label_color}
                onChange={(value) => updateTapeData({ cardinal_label_color: value })}
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <SliderField
                label="Font Size"
                value={tapeData.label_font_size}
                min={6}
                max={36}
                step={1}
                valueDisplay={`${tapeData.label_font_size}px`}
                onSliderChange={(value) => updateTapeData({ label_font_size: value })}
              />
              <SliderField
                label="Offset"
                value={tapeData.label_offset}
                min={0}
                max={20}
                step={1}
                valueDisplay={`${tapeData.label_offset}px`}
                onSliderChange={(value) => updateTapeData({ label_offset: value })}
              />
            </div>
          </div>

          {/* Indicator */}
          <div className="space-y-4">
            <SectionHeading icon={Target} title="Indicator" />
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">Show Indicator</span>
              <ToggleField checked={tapeData.show_indicator} onCheckedChange={(checked) => updateTapeData({ show_indicator: checked })} />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <SelectField
                label="Style"
                value={tapeData.indicator_style}
                onValueChange={(value) => updateTapeData({ indicator_style: value })}
                options={INDICATOR_STYLE_OPTIONS}
              />
              <SelectField
                label="Placement"
                value={tapeData.indicator_placement}
                onValueChange={(value) => updateTapeData({ indicator_placement: value })}
                options={INDICATOR_PLACEMENT_OPTIONS}
                disabled={!isChevronIndicator}
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <ColorField label="Indicator Color" value={tapeData.indicator_color} onChange={(value) => updateTapeData({ indicator_color: value })} />
              <SliderField
                label="Indicator Size"
                value={tapeData.indicator_size}
                min={4}
                max={40}
                step={1}
                valueDisplay={`${tapeData.indicator_size}px`}
                onSliderChange={(value) => updateTapeData({ indicator_size: value })}
              />
            </div>
          </div>
        </>
      )}
    </>
  )
}

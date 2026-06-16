import { Compass, Ruler, Type, Target } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { ColorField, SliderField, ToggleField, SelectField } from '../widgetFormControls'
import { SectionHeading } from '../widgetEditorSections'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { useMemo } from 'react'

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

/**
 * Renders heading tape display controls: tape scale, ticks, labels, indicator.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 */
export default function HeadingTapeDisplaySection({ widget, updateWidgetData }) {
  const tapeData = useMemo(() => widget.data.display_variants?.heading_tape ?? {}, [widget.data.display_variants?.heading_tape])
  const updateTape = useDisplayVariantUpdater(widget, 'heading_tape', tapeData, updateWidgetData)
  const availableFonts = useAvailableFonts()
  const showMajorTicks = tapeData.show_major_ticks
  const showMinorTicks = tapeData.show_minor_ticks
  const showMinorLabels = tapeData.show_minor_labels
  const showMajorLabels = tapeData.show_major_labels
  const isChevronIndicator = tapeData.indicator_style === 'chevron'

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={Compass} title="Tape Scale" />
        <SliderField
          label="Pixels per Degree"
          value={tapeData.pixels_per_degree}
          min={1}
          max={20}
          step={0.5}
          valueDisplay={`${tapeData.pixels_per_degree}px`}
          onSliderChange={(value) => updateTape({ pixels_per_degree: value })}
        />
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Ruler} title="Ticks" />
        <div className="grid grid-cols-2 gap-3">
          <div className="space-y-4">
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">Major Ticks</span>
              <ToggleField checked={showMajorTicks} onCheckedChange={(checked) => updateTape({ show_major_ticks: checked })} />
            </div>
            <SliderField
              label="Major Length"
              value={tapeData.major_tick_length_pct}
              min={5}
              max={100}
              step={1}
              disabled={!showMajorTicks}
              valueDisplay={`${tapeData.major_tick_length_pct}%`}
              onSliderChange={(value) => updateTape({ major_tick_length_pct: value })}
            />
            <SliderField
              label="Major Thickness"
              value={tapeData.major_tick_thickness}
              min={0.5}
              max={8}
              step={0.5}
              disabled={!showMajorTicks}
              valueDisplay={`${tapeData.major_tick_thickness}px`}
              onSliderChange={(value) => updateTape({ major_tick_thickness: value })}
            />
          </div>
          <div className="space-y-4">
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">Minor Ticks</span>
              <ToggleField checked={showMinorTicks} onCheckedChange={(checked) => updateTape({ show_minor_ticks: checked })} />
            </div>
            <SliderField
              label="Minor Length"
              value={tapeData.minor_tick_length_pct}
              min={5}
              max={100}
              step={1}
              disabled={!showMinorTicks}
              valueDisplay={`${tapeData.minor_tick_length_pct}%`}
              onSliderChange={(value) => updateTape({ minor_tick_length_pct: value })}
            />
            <SliderField
              label="Minor Thickness"
              value={tapeData.minor_tick_thickness}
              min={0.5}
              max={8}
              step={0.5}
              disabled={!showMinorTicks}
              valueDisplay={`${tapeData.minor_tick_thickness}px`}
              onSliderChange={(value) => updateTape({ minor_tick_thickness: value })}
            />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField label="Tick Color" value={tapeData.tick_color} onChange={(value) => updateTape({ tick_color: value })} />
          <ColorField label="Cardinal Color" value={tapeData.cardinal_tick_color} onChange={(value) => updateTape({ cardinal_tick_color: value })} />
        </div>
        <SelectField
          label="Alignment"
          value={tapeData.tick_alignment}
          onValueChange={(value) => updateTape({ tick_alignment: value })}
          options={ALIGNMENT_OPTIONS}
        />
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Type} title="Labels" />
        <FontSelectField
          label="Label Font"
          value={tapeData.label_font}
          onValueChange={(value) => updateTape({ label_font: value })}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
        />
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[9px] text-muted-foreground uppercase font-bold">Minor Labels</span>
          <ToggleField checked={showMinorLabels} onCheckedChange={(checked) => updateTape({ show_minor_labels: checked })} />
        </div>
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[9px] text-muted-foreground uppercase font-bold">Major Labels</span>
          <ToggleField checked={showMajorLabels} onCheckedChange={(checked) => updateTape({ show_major_labels: checked })} />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField label="Label Color" value={tapeData.label_color} onChange={(value) => updateTape({ label_color: value })} />
          <ColorField
            label="Cardinal Color"
            value={tapeData.cardinal_label_color}
            onChange={(value) => updateTape({ cardinal_label_color: value })}
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
            onSliderChange={(value) => updateTape({ label_font_size: value })}
          />
          <SliderField
            label="Offset"
            value={tapeData.label_offset}
            min={0}
            max={20}
            step={1}
            valueDisplay={`${tapeData.label_offset}px`}
            onSliderChange={(value) => updateTape({ label_offset: value })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Target} title="Indicator" />
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[9px] text-muted-foreground uppercase font-bold">Show Indicator</span>
          <ToggleField checked={tapeData.show_indicator} onCheckedChange={(checked) => updateTape({ show_indicator: checked })} />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SelectField
            label="Style"
            value={tapeData.indicator_style}
            onValueChange={(value) => updateTape({ indicator_style: value })}
            options={INDICATOR_STYLE_OPTIONS}
          />
          <SelectField
            label="Placement"
            value={tapeData.indicator_placement}
            onValueChange={(value) => updateTape({ indicator_placement: value })}
            options={INDICATOR_PLACEMENT_OPTIONS}
            disabled={!isChevronIndicator}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField label="Indicator Color" value={tapeData.indicator_color} onChange={(value) => updateTape({ indicator_color: value })} />
          <SliderField
            label="Indicator Size"
            value={tapeData.indicator_size}
            min={4}
            max={40}
            step={1}
            valueDisplay={`${tapeData.indicator_size}px`}
            onSliderChange={(value) => updateTape({ indicator_size: value })}
          />
        </div>
      </div>
    </>
  )
}

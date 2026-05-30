/**
 * Heading compass tape widget editor — provides controls for all configurable
 * parameters: tape scale, ticks, labels, and indicator.
 *
 * Follows the same component patterns as RouteMapWidgetEditor and
 * ElevationWidgetEditor (shadcn/ui controls, section headings, live preview).
 *
 * @param {object} props
 * @param {object} props.widget - Widget configuration object.
 * @param {Function} props.updateWidgetData - Updates widget data immutably.
 * @returns {JSX.Element} Rendered editor controls.
 */

import { Compass, Ruler, Type, Target } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { getFontFamilyName } from '@/lib/fonts'
import { ColorField, SliderField, ToggleField, SelectField } from './widgetFormControls'
import { SectionHeading } from './widgetEditorSections'

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

export default function HeadingWidgetEditor({ widget, updateWidgetData }) {
  const data = widget.data ?? {}
  const systemFonts = useAvailableFonts()
  const showMajorTicks = data.show_major_ticks !== false
  const showMinorTicks = data.show_minor_ticks !== false
  const showMinorLabels = (data.show_minor_labels ?? data.show_numeric_labels) !== false
  const showMajorLabels = (data.show_major_labels ?? data.show_cardinal_labels) !== false
  const isChevronIndicator = (data.indicator_style ?? 'chevron') === 'chevron'

  return (
    <>
      {/* Tape Scale */}
      <div className="space-y-4">
        <SectionHeading icon={Compass} title="Tape Scale" />
        <SliderField
          label="Pixels per Degree"
          value={data.pixels_per_degree ?? 5}
          min={1}
          max={20}
          step={0.5}
          valueDisplay={`${data.pixels_per_degree ?? 5}px`}
          onSliderChange={(value) => updateWidgetData(widget.id, { pixels_per_degree: value })}
        />
      </div>

      {/* Ticks */}
      <div className="space-y-4">
        <SectionHeading icon={Ruler} title="Ticks" />
        <div className="grid grid-cols-2 gap-3">
          <div className="space-y-4">
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">Major Ticks</span>
              <ToggleField checked={showMajorTicks} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_major_ticks: checked })} />
            </div>
            <SliderField
              label="Major Length"
              value={data.major_tick_length_pct ?? 40}
              min={5}
              max={100}
              step={1}
              disabled={!showMajorTicks}
              valueDisplay={`${data.major_tick_length_pct ?? 40}%`}
              onSliderChange={(value) => updateWidgetData(widget.id, { major_tick_length_pct: value })}
            />
            <SliderField
              label="Major Thickness"
              value={data.major_tick_thickness ?? 2}
              min={0.5}
              max={8}
              step={0.5}
              disabled={!showMajorTicks}
              valueDisplay={`${data.major_tick_thickness ?? 2}px`}
              onSliderChange={(value) => updateWidgetData(widget.id, { major_tick_thickness: value })}
            />
          </div>
          <div className="space-y-4">
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">Minor Ticks</span>
              <ToggleField checked={showMinorTicks} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_minor_ticks: checked })} />
            </div>
            <SliderField
              label="Minor Length"
              value={data.minor_tick_length_pct ?? 20}
              min={5}
              max={100}
              step={1}
              disabled={!showMinorTicks}
              valueDisplay={`${data.minor_tick_length_pct ?? 20}%`}
              onSliderChange={(value) => updateWidgetData(widget.id, { minor_tick_length_pct: value })}
            />
            <SliderField
              label="Minor Thickness"
              value={data.minor_tick_thickness ?? 2}
              min={0.5}
              max={8}
              step={0.5}
              disabled={!showMinorTicks}
              valueDisplay={`${data.minor_tick_thickness ?? 2}px`}
              onSliderChange={(value) => updateWidgetData(widget.id, { minor_tick_thickness: value })}
            />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Tick Color"
            value={data.tick_color || '#ffffff'}
            onChange={(value) => updateWidgetData(widget.id, { tick_color: value })}
          />
          <ColorField
            label="Cardinal Color"
            value={data.cardinal_tick_color || '#ff0000'}
            onChange={(value) => updateWidgetData(widget.id, { cardinal_tick_color: value })}
          />
        </div>
        <SelectField
          label="Alignment"
          value={data.tick_alignment ?? 'below'}
          onValueChange={(value) => updateWidgetData(widget.id, { tick_alignment: value })}
          options={ALIGNMENT_OPTIONS}
        />
      </div>

      {/* Labels */}
      <div className="space-y-4">
        <SectionHeading icon={Type} title="Labels" />
        <FontSelectField
          label="Label Font"
          value={data.label_font || data.label_font_family || 'Arial.ttf'}
          onValueChange={(value) => updateWidgetData(widget.id, { label_font: value, label_font_family: getFontFamilyName(value) })}
          systemFonts={systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
        />
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[9px] text-muted-foreground uppercase font-bold">Minor Labels</span>
          <ToggleField checked={showMinorLabels} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_minor_labels: checked })} />
        </div>
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[9px] text-muted-foreground uppercase font-bold">Major Labels</span>
          <ToggleField checked={showMajorLabels} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_major_labels: checked })} />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Label Color"
            value={data.label_color || data.numeric_label_color || data.minor_label_color || '#cccccc'}
            onChange={(value) => updateWidgetData(widget.id, { label_color: value })}
          />
          <ColorField
            label="Cardinal Color"
            value={data.cardinal_label_color || data.major_label_color || '#ff0000'}
            onChange={(value) => updateWidgetData(widget.id, { cardinal_label_color: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Font Size"
            value={data.label_font_size ?? 12}
            min={6}
            max={36}
            step={1}
            valueDisplay={`${data.label_font_size ?? 12}px`}
            onSliderChange={(value) => updateWidgetData(widget.id, { label_font_size: value })}
          />
          <SliderField
            label="Offset"
            value={data.label_offset ?? 4}
            min={0}
            max={20}
            step={1}
            valueDisplay={`${data.label_offset ?? 4}px`}
            onSliderChange={(value) => updateWidgetData(widget.id, { label_offset: value })}
          />
        </div>
      </div>

      {/* Indicator */}
      <div className="space-y-4">
        <SectionHeading icon={Target} title="Indicator" />
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[9px] text-muted-foreground uppercase font-bold">Show Indicator</span>
          <ToggleField
            checked={data.show_indicator !== false}
            onCheckedChange={(checked) => updateWidgetData(widget.id, { show_indicator: checked })}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SelectField
            label="Style"
            value={data.indicator_style ?? 'chevron'}
            onValueChange={(value) => updateWidgetData(widget.id, { indicator_style: value })}
            options={INDICATOR_STYLE_OPTIONS}
          />
          <SelectField
            label="Placement"
            value={data.indicator_placement ?? 'top'}
            onValueChange={(value) => updateWidgetData(widget.id, { indicator_placement: value })}
            options={INDICATOR_PLACEMENT_OPTIONS}
            disabled={!isChevronIndicator}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Indicator Color"
            value={data.indicator_color || '#ff0000'}
            onChange={(value) => updateWidgetData(widget.id, { indicator_color: value })}
          />
          <SliderField
            label="Indicator Size"
            value={data.indicator_size ?? 10}
            min={4}
            max={40}
            step={1}
            valueDisplay={`${data.indicator_size ?? 10}px`}
            onSliderChange={(value) => updateWidgetData(widget.id, { indicator_size: value })}
          />
        </div>
      </div>
    </>
  )
}

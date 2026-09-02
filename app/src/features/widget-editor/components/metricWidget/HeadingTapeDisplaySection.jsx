import { Compass, Ruler, Type, Target } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import { SectionHeading } from '@/components/ui/section-heading'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { ColorField, SizeSlider, SliderField, ToggleField, SelectField } from '../widgetFormControls'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { translateOptions } from '@/i18n'

const ALIGNMENT_OPTIONS = [
  { value: 'below', labelKey: 'widget-editor.alignmentBelow', defaultLabel: 'Below' },
  { value: 'centered', labelKey: 'widget-editor.alignmentCentered', defaultLabel: 'Centered' },
]

const INDICATOR_STYLE_OPTIONS = [
  { value: 'chevron', labelKey: 'widget-editor.indicatorStyleChevron', defaultLabel: 'Chevron' },
  { value: 'highlight_bar', labelKey: 'widget-editor.indicatorStyleHighlightBar', defaultLabel: 'Highlight Bar' },
]

const INDICATOR_PLACEMENT_OPTIONS = [
  { value: 'top', labelKey: 'widget-editor.top', defaultLabel: 'Top' },
  { value: 'bottom', labelKey: 'widget-editor.bottom', defaultLabel: 'Bottom' },
  { value: 'both', labelKey: 'widget-editor.both', defaultLabel: 'Both' },
]

/**
 * Renders heading tape display controls: tape scale, ticks, labels, indicator.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 */
export default function HeadingTapeDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  const tapeData = useMemo(() => widget.data.display_variants?.heading_tape ?? {}, [widget.data.display_variants?.heading_tape])
  const updateTape = useDisplayVariantUpdater(widget, 'heading_tape', tapeData, updateWidgetData)
  const updateTapeSize = useDisplayVariantUpdater(widget, 'heading_tape', tapeData, updateWidgetSize)
  const availableFonts = useAvailableFonts()
  const showMajorTicks = tapeData.show_major_ticks
  const showMinorTicks = tapeData.show_minor_ticks
  const showMinorLabels = tapeData.show_minor_labels
  const showMajorLabels = tapeData.show_major_labels
  const isChevronIndicator = tapeData.indicator_style === 'chevron'

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={Compass} title={t('widget-editor.tapeScale', 'Tape Scale')} />
        <SizeSlider
          label={t('widget-editor.pixelsPerDegree', 'Pixels per Degree')}
          value={tapeData.pixels_per_degree}
          min={1}
          max={20}
          step={0.5}
          valueDisplay={`${tapeData.pixels_per_degree}px`}
          onChange={(value) => updateTapeSize({ pixels_per_degree: value })}
          onCommit={() => commitWidgetSize(widget.id)}
        />
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Ruler} title={t('widget-editor.ticks', 'Ticks')} />
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-4">
            <div className="flex items-center justify-between gap-2 px-1 pb-2">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.majorTicks', 'Major Ticks')}</span>
              <ToggleField checked={showMajorTicks} onCheckedChange={(checked) => updateTape({ show_major_ticks: checked })} />
            </div>
            <SliderField
              label={t('widget-editor.majorLength', 'Major Length')}
              value={tapeData.major_tick_length_pct}
              min={5}
              max={100}
              step={1}
              disabled={!showMajorTicks}
              valueDisplay={`${tapeData.major_tick_length_pct}%`}
              onSliderChange={(value) => updateTapeSize({ major_tick_length_pct: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
            <SliderField
              label={t('widget-editor.majorThickness', 'Major Thickness')}
              value={tapeData.major_tick_thickness}
              min={0.5}
              max={8}
              step={0.5}
              disabled={!showMajorTicks}
              integerDisplay
              valueDisplay={`${tapeData.major_tick_thickness}px`}
              onSliderChange={(value) => updateTapeSize({ major_tick_thickness: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          </div>
          <div className="space-y-4">
            <div className="flex items-center justify-between gap-2 px-1 pb-2">
              <span className="text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.minorTicks', 'Minor Ticks')}</span>
              <ToggleField checked={showMinorTicks} onCheckedChange={(checked) => updateTape({ show_minor_ticks: checked })} />
            </div>
            <SliderField
              label={t('widget-editor.minorLength', 'Minor Length')}
              value={tapeData.minor_tick_length_pct}
              min={5}
              max={100}
              step={1}
              disabled={!showMinorTicks}
              valueDisplay={`${tapeData.minor_tick_length_pct}%`}
              onSliderChange={(value) => updateTapeSize({ minor_tick_length_pct: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
            <SliderField
              label={t('widget-editor.minorThickness', 'Minor Thickness')}
              value={tapeData.minor_tick_thickness}
              min={0.5}
              max={8}
              step={0.5}
              disabled={!showMinorTicks}
              integerDisplay
              valueDisplay={`${tapeData.minor_tick_thickness}px`}
              onSliderChange={(value) => updateTapeSize({ minor_tick_thickness: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.tickColor', 'Tick Color')}
            value={tapeData.tick_color}
            onChange={(value) => updateTape({ tick_color: value })}
          />
          <ColorField
            label={t('widget-editor.cardinalColor', 'Cardinal Color')}
            value={tapeData.cardinal_tick_color}
            onChange={(value) => updateTape({ cardinal_tick_color: value })}
          />
        </div>
        <SelectField
          label={t('widget-editor.alignment', 'Alignment')}
          value={tapeData.tick_alignment}
          onValueChange={(value) => updateTape({ tick_alignment: value })}
          options={translateOptions(ALIGNMENT_OPTIONS, t)}
        />
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Type} title={t('widget-editor.labels', 'Labels')} />
        <FontSelectField
          label={t('widget-editor.labelFont', 'Label Font')}
          value={tapeData.label_font}
          onValueChange={(value) => updateTape({ label_font: value })}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
        />
        <div className="grid grid-cols-2 gap-4 py-2">
          <div className="flex items-center justify-between gap-2 px-1">
            <span className="text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.minorLabels', 'Minor Labels')}</span>
            <ToggleField checked={showMinorLabels} onCheckedChange={(checked) => updateTape({ show_minor_labels: checked })} />
          </div>
          <div className="flex items-center justify-between gap-2 px-1">
            <span className="text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.majorLabels', 'Major Labels')}</span>
            <ToggleField checked={showMajorLabels} onCheckedChange={(checked) => updateTape({ show_major_labels: checked })} />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.labelColor', 'Label Color')}
            value={tapeData.label_color}
            onChange={(value) => updateTape({ label_color: value })}
          />
          <ColorField
            label={t('widget-editor.cardinalColor', 'Cardinal Color')}
            value={tapeData.cardinal_label_color}
            onChange={(value) => updateTape({ cardinal_label_color: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SizeSlider
            label={t('widget-editor.fontSize', 'Font Size')}
            value={tapeData.label_font_size}
            min={6}
            max={36}
            step={1}
            valueDisplay={`${tapeData.label_font_size}px`}
            onChange={(value) => updateTapeSize({ label_font_size: value })}
            onCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            label={t('widget-editor.offset', 'Offset')}
            value={tapeData.label_offset}
            min={0}
            max={20}
            step={1}
            integerDisplay
            valueDisplay={`${tapeData.label_offset}px`}
            onSliderChange={(value) => updateTapeSize({ label_offset: value })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Target} title={t('widget-editor.indicator', 'Indicator')} />
        <div className="flex items-center justify-between gap-2 px-1">
          <span className="text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.showIndicator', 'Show Indicator')}</span>
          <ToggleField checked={tapeData.show_indicator} onCheckedChange={(checked) => updateTape({ show_indicator: checked })} />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SelectField
            label={t('widget-editor.style', 'Style')}
            value={tapeData.indicator_style}
            onValueChange={(value) => updateTape({ indicator_style: value })}
            options={translateOptions(INDICATOR_STYLE_OPTIONS, t)}
          />
          <SelectField
            label={t('widget-editor.placement', 'Placement')}
            value={tapeData.indicator_placement}
            onValueChange={(value) => updateTape({ indicator_placement: value })}
            options={translateOptions(INDICATOR_PLACEMENT_OPTIONS, t)}
            disabled={!isChevronIndicator}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.indicatorColor', 'Indicator Color')}
            value={tapeData.indicator_color}
            onChange={(value) => updateTape({ indicator_color: value })}
          />
          <SizeSlider
            label={t('widget-editor.indicatorSize', 'Indicator Size')}
            value={tapeData.indicator_size}
            min={4}
            max={40}
            step={1}
            valueDisplay={`${tapeData.indicator_size}px`}
            onChange={(value) => updateTapeSize({ indicator_size: value })}
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
      </div>
    </>
  )
}

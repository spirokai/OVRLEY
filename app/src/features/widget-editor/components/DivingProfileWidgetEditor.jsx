/**
 * Supports widget editing flows related to diving profile widget editor.
 */

import { Mountain, Palette } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { ColorField, SizeSlider, SliderField, ToggleField } from './widgetFormControls'
import { DimensionsSection } from './widgetEditorSections'
import PlotMarkerSection from './PlotMarkerSection'
import { Label } from '@/components/ui/label'
import { useTranslation } from 'react-i18next'

/**
 * Renders the diving profile widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function DivingProfileWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize, setNumericField }) {
  const { t } = useTranslation()
  const lineWidth = widget.data.completed_line_width
  const remainingLineOpacity = widget.data.remaining_line_opacity
  const yScale = widget.data.y_scale
  const simplifyTolerance = widget.data.simplify_tolerance_px
  const targetDensity = widget.data.target_density
  const labelFontSize = widget.data.point_label.font_size
  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />
      <div className="space-y-4">
        <SectionHeading icon={Palette} title={t('widget-editor.lineStyling', 'Line Styling')} />
        <SliderField
          editable
          label={t('widget-editor.thickness', 'Thickness')}
          value={lineWidth}
          min={1}
          max={20}
          step={1}
          integerDisplay
          valueDisplay={`${lineWidth}`}
          suffix="px"
          onSliderChange={(value) =>
            updateWidgetSize(widget.id, {
              completed_line_width: value,
              remaining_line_width: value,
            })
          }
          onSliderCommit={() => commitWidgetSize(widget.id)}
        />

        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.smoothing', 'Smoothing')}
            value={simplifyTolerance}
            min={0}
            max={4}
            step={0.05}
            valueDisplay={`${simplifyTolerance.toFixed(2)}`}
            suffix="px"
            onSliderChange={(value) =>
              updateWidgetSize(widget.id, {
                simplify_tolerance_px: Number(value.toFixed(2)),
              })
            }
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            editable
            label={t('widget-editor.profileDetail', 'Profile Detail')}
            value={targetDensity}
            min={0.25}
            max={1.5}
            step={0.05}
            valueDisplay={`${targetDensity.toFixed(2)}`}
            suffix="x"
            onSliderChange={(value) =>
              updateWidgetSize(widget.id, {
                target_density: Number(value.toFixed(2)),
              })
            }
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.finishedColor', 'Finished Color')}
            value={widget.data.completed_line_color}
            onChange={(value) =>
              updateWidgetData(widget.id, {
                completed_line_color: value,
              })
            }
          />
          <ColorField
            label={t('widget-editor.remainingColor', 'Remaining Color')}
            value={widget.data.remaining_line_color}
            onChange={(value) => updateWidgetData(widget.id, { remaining_line_color: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.finishedOpacity', 'Finished Opacity')}
            value={widget.data.completed_line_opacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${widget.data.completed_line_opacity}`}
            suffix="%"
            onSliderChange={(value) => updateWidgetData(widget.id, { completed_line_opacity: value })}
          />
          <SliderField
            editable
            label={t('widget-editor.remainingOpacity', 'Remaining Opacity')}
            value={remainingLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${remainingLineOpacity}`}
            suffix="%"
            onSliderChange={(value) => updateWidgetData(widget.id, { remaining_line_opacity: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.verticalScale', 'Vertical Scale')}
            value={yScale}
            min={0.2}
            max={4}
            step={0.05}
            valueDisplay={`${yScale.toFixed(2)}`}
            suffix="x"
            onSliderChange={(value) =>
              updateWidgetSize(widget.id, {
                y_scale: Number(value.toFixed(2)),
              })
            }
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />

          <div className="flex items-center justify-between gap-2 px-1 pt-6">
            <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">
              {t('widget-editor.showFullActivity', 'Show Full Activity')}
            </Label>
            <ToggleField
              checked={widget.data.show_full_activity}
              onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_activity: checked })}
            />
          </div>
        </div>
        <div className="space-y-4">
          <SectionHeading icon={Palette} title={t('widget-editor.decompressionCeiling')} />
          <div className="grid grid-cols-2 gap-4">
            <ColorField
              label={t('widget-editor.ceilingLineColor')}
              value={widget.data.ceiling_line_color}
              onChange={(value) => updateWidgetData(widget.id, { ceiling_line_color: value })}
            />
            <ColorField
              label={t('widget-editor.ceilingAreaColor')}
              value={widget.data.ceiling_area_color}
              onChange={(value) => updateWidgetData(widget.id, { ceiling_area_color: value })}
            />
            <SliderField
              editable
              label={t('widget-editor.thickness')}
              value={widget.data.ceiling_line_width}
              min={0}
              max={20}
              step={1}
              suffix="px"
              valueDisplay={String(widget.data.ceiling_line_width)}
              onSliderChange={(value) => updateWidgetSize(widget.id, { ceiling_line_width: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
            <SliderField
              editable
              label={t('widget-editor.ceilingLineOpacity')}
              value={widget.data.ceiling_line_opacity}
              min={0}
              max={100}
              step={1}
              suffix="%"
              valueDisplay={String(widget.data.ceiling_line_opacity)}
              onSliderChange={(value) => updateWidgetData(widget.id, { ceiling_line_opacity: value })}
            />
            <SliderField
              editable
              label={t('widget-editor.ceilingAreaOpacity')}
              value={widget.data.ceiling_area_opacity}
              min={0}
              max={100}
              step={1}
              suffix="%"
              valueDisplay={String(widget.data.ceiling_area_opacity)}
              onSliderChange={(value) => updateWidgetData(widget.id, { ceiling_area_opacity: value })}
            />
          </div>
        </div>
      </div>
      <PlotMarkerSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
      />

      <div className="space-y-4">
        <SectionHeading icon={Mountain} title={t('widget-editor.labels', 'Labels')} />
        <SizeSlider
          label={t('widget-editor.labelSize', 'Label Size')}
          value={labelFontSize}
          min={5}
          max={50}
          step={1}
          valueDisplay={`${labelFontSize}`}
          suffix="px"
          onChange={(value) => updateWidgetSize(widget.id, { point_label: { ...widget.data.point_label, font_size: value } })}
          onCommit={() => commitWidgetSize(widget.id)}
        />

        <div className="grid grid-cols-2 gap-4 pt-2">
          <div className="flex flex-col gap-4">
            <div className="flex justify-between items-center gap-2 pl-1 pb-3">
              <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.metricLabel', 'Metric Label')}</Label>
              <ToggleField
                label={t('widget-editor.labelMetric', 'Label Metric')}
                checked={widget.data.show_depth_metric}
                onCheckedChange={(checked) =>
                  updateWidgetData(widget.id, {
                    show_depth_metric: checked,
                  })
                }
              />
            </div>
            <SliderField
              editable
              label={t('widget-editor.metricOffsetX', 'Metric Offset X')}
              disabled={!widget.data.show_depth_metric}
              value={widget.data.metric_label_offset_x}
              min={-100}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.metric_label_offset_x}`}
              suffix="px"
              onSliderChange={(value) => updateWidgetSize(widget.id, { metric_label_offset_x: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
            <SliderField
              editable
              label={t('widget-editor.metricOffsetY', 'Metric Offset Y')}
              disabled={!widget.data.show_depth_metric}
              value={widget.data.metric_label_offset_y}
              min={-100}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.metric_label_offset_y}`}
              suffix="px"
              onSliderChange={(value) => updateWidgetSize(widget.id, { metric_label_offset_y: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          </div>
          <div className="flex flex-col gap-4">
            <div className="flex justify-between items-center gap-2 pl-1 pb-3">
              <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.imperialLabel', 'Imperial Label')}</Label>
              <ToggleField
                label={t('widget-editor.labelImperial', 'Label Imperial')}
                checked={widget.data.show_depth_imperial}
                onCheckedChange={(checked) =>
                  updateWidgetData(widget.id, {
                    show_depth_imperial: checked,
                  })
                }
              />
            </div>
            <SliderField
              editable
              label={t('widget-editor.imperialOffsetX', 'Imperial Offset X')}
              disabled={!widget.data.show_depth_imperial}
              value={widget.data.imperial_label_offset_x}
              min={-100}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.imperial_label_offset_x}`}
              suffix="px"
              onSliderChange={(value) => updateWidgetSize(widget.id, { imperial_label_offset_x: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
            <SliderField
              editable
              label={t('widget-editor.imperialOffsetY', 'Imperial Offset Y')}
              disabled={!widget.data.show_depth_imperial}
              value={widget.data.imperial_label_offset_y}
              min={-100}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.imperial_label_offset_y}`}
              suffix="px"
              onSliderChange={(value) => updateWidgetSize(widget.id, { imperial_label_offset_y: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          </div>
        </div>
      </div>
    </>
  )
}

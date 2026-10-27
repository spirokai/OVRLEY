/**
 * Supports widget editing flows related to route map widget editor.
 */

import { Palette } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { ColorField, SliderField, ToggleField } from './widgetFormControls'
import { DimensionsSection } from './widgetEditorSections'
import PlotMarkerSection from './PlotMarkerSection'
import { getThemeColor } from '@/lib/color-utils'
import { Label } from '@/components/ui/label'
import { useTranslation } from 'react-i18next'
import { normalizeRotation } from '@/features/wasm-widgets/utils/widgetGestureGeometry'

/**
 * Renders the route map widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function RouteMapWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize, setNumericField }) {
  const { t } = useTranslation()
  const lineWidth = widget.data.completed_line_width ?? widget.data.remaining_line_width
  const completedLineOpacity = widget.data.completed_line_opacity
  const remainingLineOpacity = widget.data.remaining_line_opacity
  const rotation = normalizeRotation(widget.data.rotation)
  const simplifyTolerance = widget.data.simplify_tolerance_px
  const targetDensity = widget.data.target_density

  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />

      <SliderField
        editable
        label={t('widget-editor.mapRotation', 'Map Rotation')}
        value={rotation}
        min={0}
        max={360}
        step={1}
        integerDisplay
        suffix="°"
        onSliderChange={(rawValue) => updateWidgetSize(widget.id, { rotation: rawValue })}
        onSliderCommit={() => commitWidgetSize(widget.id)}
      />

      <div className="space-y-4">
        <SectionHeading icon={Palette} title={t('widget-editor.lineStyling', 'Line Styling')} />
        <SliderField
          editable
          label={t('widget-editor.thickness', 'Thickness')}
          value={lineWidth}
          min={0}
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
            label={t('widget-editor.routeDetail', 'Route Detail')}
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
            value={widget.data.completed_line_color || getThemeColor('ice')}
            onChange={(value) =>
              updateWidgetData(widget.id, {
                completed_line_color: value,
                color: value,
              })
            }
          />
          <ColorField
            label={t('widget-editor.remainingColor', 'Remaining Color')}
            value={widget.data.remaining_line_color || getThemeColor('teal')}
            onChange={(value) => updateWidgetData(widget.id, { remaining_line_color: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.finishedOpacity', 'Finished Opacity')}
            value={completedLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${completedLineOpacity}`}
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
          <div className="flex items-center justify-between gap-2 pl-1 pt-2 pb-2">
            <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">
              {t('widget-editor.showFullActivity', 'Show Full Activity')}
            </Label>
            <ToggleField
              checked={widget.data.show_full_activity}
              onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_activity: checked })}
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
    </>
  )
}

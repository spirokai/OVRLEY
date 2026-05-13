/**
 * Supports widget editing flows related to route map widget editor.
 */

import { Map, Palette } from 'lucide-react'
import { ColorField, SliderField, ToggleField } from './widgetFormControls'
import { DimensionsSection, SectionHeading } from './widgetEditorSections'
import { getThemeColor } from '@/lib/theme'
import { Label } from '@/components/ui/label'

/**
 * Renders the route map widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function RouteMapWidgetEditor({ widget, updateWidgetData, setNumericField }) {
  const lineWidth = widget.data.completed_line_width ?? widget.data.remaining_line_width ?? 6
  const completedLineOpacity = widget.data.completed_line_opacity ?? 100
  const remainingLineOpacity = widget.data.remaining_line_opacity ?? 35
  const markerSize = widget.data.marker_size ?? 18
  const markerOpacity = widget.data.marker_opacity ?? 100
  const rotation = widget.data.rotation ?? 0
  const simplifyTolerance = widget.data.simplify_tolerance_px ?? 1
  const targetDensity = widget.data.target_density ?? 1

  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />

      <SliderField
        label="Map Rotation"
        value={rotation}
        min={-180}
        max={180}
        step={1}
        valueDisplay={`${rotation}°`}
        onSliderChange={(rawValue) =>
          setNumericField(widget.id, 'rotation', rawValue, {
            min: -180,
            max: 180,
          })
        }
      />

      <div className="space-y-4">
        <SectionHeading icon={Palette} title="Line Styling" />
        <div className="flex items-center justify-between gap-2 px-1">
          <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">Render Full Activity</Label>
          <ToggleField
            checked={widget.data.show_full_activity ?? false}
            onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_activity: checked })}
          />
        </div>
        <SliderField
          label="Thickness"
          value={lineWidth}
          min={0}
          max={20}
          step={1}
          valueDisplay={`${lineWidth}px`}
          onSliderChange={(value) =>
            updateWidgetData(widget.id, {
              completed_line_width: value,
              remaining_line_width: value,
            })
          }
        />
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Finished Color"
            value={widget.data.completed_line_color || getThemeColor('ice')}
            onChange={(value) =>
              updateWidgetData(widget.id, {
                completed_line_color: value,
                color: value,
              })
            }
          />
          <ColorField
            label="Remaining Color"
            value={widget.data.remaining_line_color || getThemeColor('teal')}
            onChange={(value) => updateWidgetData(widget.id, { remaining_line_color: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Finished Opacity"
            value={completedLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${completedLineOpacity}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { completed_line_opacity: value })}
          />
          <SliderField
            label="Remaining Opacity"
            value={remainingLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${remainingLineOpacity}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { remaining_line_opacity: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Smoothing"
            value={simplifyTolerance}
            min={0}
            max={4}
            step={0.05}
            valueDisplay={`${simplifyTolerance.toFixed(2)}px`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, {
                simplify_tolerance_px: Number(value.toFixed(2)),
              })
            }
          />
          <SliderField
            label="Data Density"
            value={targetDensity}
            min={0.25}
            max={1.5}
            step={0.05}
            valueDisplay={`${targetDensity.toFixed(2)}x`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, {
                target_density: Number(value.toFixed(2)),
              })
            }
          />
        </div>
      </div>
      <div className="space-y-4">
        <SectionHeading icon={Map} title="Marker" />
        <SliderField
          label=" Size"
          value={markerSize}
          min={0}
          max={50}
          step={1}
          valueDisplay={`${markerSize}px`}
          onSliderChange={(value) => updateWidgetData(widget.id, { marker_size: value })}
        />
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Color"
            value={widget.data.marker_color || getThemeColor('aqua')}
            onChange={(value) => updateWidgetData(widget.id, { marker_color: value })}
          />
          <SliderField
            label="Opacity"
            value={markerOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${markerOpacity}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { marker_opacity: value })}
          />
        </div>
      </div>
    </>
  )
}

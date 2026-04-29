/**
 * Supports widget editing flows related to elevation widget editor.
 */

import { Mountain, Palette } from 'lucide-react'
import {
  ColorField,
  NumberField,
  SliderField,
  ToggleField,
} from './widgetFormControls'
import { DimensionsSection, SectionHeading } from './widgetEditorSections'
import { getThemeColor } from '@/lib/theme'

/**
 * Renders the elevation widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function ElevationWidgetEditor({
  widget,
  updateWidgetData,
  setNumericField,
}) {
  const lineWidth =
    widget.data.completed_line_width ?? widget.data.remaining_line_width ?? 6
  const completedLineOpacity = widget.data.completed_line_opacity ?? 100
  const remainingLineOpacity = widget.data.remaining_line_opacity ?? 35
  const completedAreaOpacity = widget.data.area_completed_opacity ?? 24
  const remainingAreaOpacity = widget.data.area_remaining_opacity ?? 12
  const yScale = widget.data.y_scale ?? 1
  const simplifyTolerance = widget.data.simplify_tolerance_px ?? 1
  const targetDensity = widget.data.target_density ?? 0.75

  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />
      <div className="space-y-4">
        <SectionHeading icon={Palette} title="Line Styling" />
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
            onChange={(value) =>
              updateWidgetData(widget.id, { remaining_line_color: value })
            }
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
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { completed_line_opacity: value })
            }
          />
          <SliderField
            label="Remaining Opacity"
            value={remainingLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${remainingLineOpacity}%`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { remaining_line_opacity: value })
            }
          />
        </div>
        <SliderField
          label="Vertical Scale"
          value={yScale}
          min={0.1}
          max={3}
          step={0.05}
          valueDisplay={`${yScale.toFixed(2)}x`}
          onSliderChange={(value) =>
            updateWidgetData(widget.id, {
              y_scale: Number(value.toFixed(2)),
            })
          }
        />
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
        <div className="space-y-4">
          <SectionHeading icon={Palette} title="Area Styling" />
          <div className="grid grid-cols-2 gap-3">
            <ColorField
              label="Finished Color"
              value={widget.data.area_completed_color || getThemeColor('ice')}
              onChange={(value) =>
                updateWidgetData(widget.id, { area_completed_color: value })
              }
            />
            <ColorField
              label="Remaining Color"
              value={widget.data.area_remaining_color || getThemeColor('teal')}
              onChange={(value) =>
                updateWidgetData(widget.id, { area_remaining_color: value })
              }
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <SliderField
              label="Finished Opacity"
              value={completedAreaOpacity}
              min={0}
              max={100}
              step={1}
              valueDisplay={`${completedAreaOpacity}%`}
              onSliderChange={(value) =>
                updateWidgetData(widget.id, { area_completed_opacity: value })
              }
            />
            <SliderField
              label="Remaining Opacity"
              value={remainingAreaOpacity}
              min={0}
              max={100}
              step={1}
              valueDisplay={`${remainingAreaOpacity}%`}
              onSliderChange={(value) =>
                updateWidgetData(widget.id, { area_remaining_opacity: value })
              }
            />
          </div>
        </div>
      </div>
      <div className="space-y-4">
        <SectionHeading icon={Mountain} title="Marker & Labels" />
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Size"
            value={widget.data.marker_size ?? 16}
            min={0}
            max={50}
            step={1}
            valueDisplay={`${widget.data.marker_size ?? 16}px`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { marker_size: value })
            }
          />
          <SliderField
            label="Opacity"
            value={widget.data.marker_opacity ?? 100}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${widget.data.marker_opacity ?? 100}%`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, { marker_opacity: value })
            }
          />
          <ColorField
            label="Color"
            value={widget.data.marker_color || getThemeColor('aqua')}
            onChange={(value) =>
              updateWidgetData(widget.id, { marker_color: value })
            }
          />
        </div>
        <ToggleField
          label="Label Metric"
          checked={widget.data.show_elevation_metric ?? true}
          onCheckedChange={(checked) =>
            updateWidgetData(widget.id, { show_elevation_metric: checked })
          }
        />
        <ToggleField
          label="Label Imperial"
          checked={widget.data.show_elevation_imperial ?? false}
          onCheckedChange={(checked) =>
            updateWidgetData(widget.id, { show_elevation_imperial: checked })
          }
        />
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label="Metric Offset X"
            value={widget.data.metric_label_offset_x ?? 0}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'metric_label_offset_x', rawValue)
            }
          />
          <NumberField
            label="Metric Offset Y"
            value={widget.data.metric_label_offset_y ?? 0}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'metric_label_offset_y', rawValue)
            }
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label="Imperial Offset X"
            value={widget.data.imperial_label_offset_x ?? 0}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'imperial_label_offset_x', rawValue)
            }
          />
          <NumberField
            label="Imperial Offset Y"
            value={widget.data.imperial_label_offset_y ?? 0}
            onChange={(rawValue) =>
              setNumericField(widget.id, 'imperial_label_offset_y', rawValue)
            }
          />
        </div>
      </div>
    </>
  )
}

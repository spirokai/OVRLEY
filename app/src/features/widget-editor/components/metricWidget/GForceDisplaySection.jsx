import { useMemo } from 'react'
import { CircleGauge, Type } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import { SectionHeading } from '@/components/ui/section-heading'
import { buildUniformResizeUpdate } from '@/features/overlay-editor/utils/widgetResizeScaling'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { ColorField, SizeSlider, SliderField } from '../widgetFormControls'

/** Standard controls for G-force geometry, paint, marker, and label styling. */
export default function GForceDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const data = useMemo(() => widget.data.display_variants.g_force, [widget.data.display_variants.g_force])
  const updateGForce = useDisplayVariantUpdater(widget, 'g_force', data, updateWidgetData)
  const updateGForceSize = useDisplayVariantUpdater(widget, 'g_force', data, updateWidgetSize)
  const availableFonts = useAvailableFonts()
  const borderMax = Math.min(Math.floor((data.diameter - 1) / 2), 8)

  const handleDiameterChange = (diameter) => {
    const frameSize = data.width * (diameter / data.diameter)
    const update = buildUniformResizeUpdate(widget, frameSize)
    if (update) updateWidgetSize(widget.id, update)
  }

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={CircleGauge} title="G-Force Plot" />
        <SizeSlider
          label="Size"
          value={data.diameter}
          min={20}
          max={600}
          valueDisplay={`${data.diameter}px`}
          onChange={handleDiameterChange}
          onCommit={() => commitWidgetSize(widget.id)}
        />
        <div className="grid grid-cols-2 gap-4">
          <ColorField label="Fill Color" value={data.fill_color} onChange={(fill_color) => updateGForce({ fill_color })} />
          <SliderField
            label="Fill Opacity"
            value={data.fill_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(data.fill_opacity * 100)}%`}
            onSliderChange={(fill_opacity) => updateGForce({ fill_opacity })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField label="Border Color" value={data.border_color} onChange={(border_color) => updateGForce({ border_color })} />
          <SliderField
            label="Border Thickness"
            value={data.border_thickness}
            min={0}
            max={borderMax}
            integerDisplay
            valueDisplay={`${data.border_thickness}px`}
            onSliderChange={(border_thickness) => updateGForceSize({ border_thickness })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField label="Marker Color" value={data.marker_color} onChange={(marker_color) => updateGForce({ marker_color })} />
          <SizeSlider
            label="Marker Size"
            value={data.marker_size}
            min={2}
            max={48}
            valueDisplay={`${data.marker_size}px`}
            onChange={(marker_size) => updateGForceSize({ marker_size })}
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label="Marker Opacity"
            value={data.marker_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(data.marker_opacity * 100)}%`}
            onSliderChange={(marker_opacity) => updateGForce({ marker_opacity })}
          />
          <SliderField
            label="Border Opacity"
            value={data.border_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(data.border_opacity * 100)}%`}
            onSliderChange={(border_opacity) => updateGForce({ border_opacity })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Type} title="Label" />
        <div className="grid grid-cols-2 gap-4">
          <FontSelectField
            label="Label Font"
            value={data.label_font}
            onValueChange={(label_font) => updateGForce({ label_font })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <SizeSlider
            label="Font Size"
            value={data.label_font_size}
            min={6}
            max={100}
            valueDisplay={`${data.label_font_size}px`}
            onChange={(label_font_size) => updateGForceSize({ label_font_size })}
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField label="Label Color" value={data.label_color} onChange={(label_color) => updateGForce({ label_color })} />
          <ColorField label="Unit Color" value={data.label_unit_color} onChange={(label_unit_color) => updateGForce({ label_unit_color })} />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label="Horizontal Offset"
            value={data.label_offset_x}
            min={-100}
            max={100}
            integerDisplay
            valueDisplay={`${data.label_offset_x}px`}
            onSliderChange={(label_offset_x) => updateGForceSize({ label_offset_x })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            label="Vertical Offset"
            value={data.label_offset_y}
            min={-100}
            max={100}
            integerDisplay
            valueDisplay={`${data.label_offset_y}px`}
            onSliderChange={(label_offset_y) => updateGForceSize({ label_offset_y })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
      </div>
    </>
  )
}

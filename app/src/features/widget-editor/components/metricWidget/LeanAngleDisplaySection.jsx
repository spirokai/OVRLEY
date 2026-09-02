import { useMemo } from 'react'
import { SlidersHorizontal, Type } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { buildUniformResizeUpdate } from '@/features/overlay-editor/utils/widgetResizeScaling'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { ColorField, SizeSlider, SliderField } from '../widgetFormControls'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { useTranslation } from 'react-i18next'

/**
 * Editor controls for the diameter-based lean-angle display type.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @returns {JSX.Element}
 */
export default function LeanAngleDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  const leanVariant = useMemo(() => widget.data.display_variants?.lean_angle ?? {}, [widget.data.display_variants?.lean_angle])
  const updateLean = useDisplayVariantUpdater(widget, 'lean_angle', leanVariant, updateWidgetData)
  const updateLeanSize = useDisplayVariantUpdater(widget, 'lean_angle', leanVariant, updateWidgetSize)

  const diameter = leanVariant.diameter
  const trackThicknessMax = Math.floor((diameter - 1) / 2)
  const borderThicknessMax = Math.min(8, Math.floor((leanVariant.track_thickness - 1) / 2))
  const availableFonts = useAvailableFonts()

  const handleDiameterChange = (nextDiameter) => {
    const update = buildUniformResizeUpdate(widget, nextDiameter)
    if (update) updateWidgetSize(widget.id, update)
  }

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={SlidersHorizontal} title={t('widget-editor.angleTrack', 'Angle Track')} />
        <div className="grid grid-cols-2 gap-4">
          <SizeSlider
            label={t('widget-editor.size', 'Size')}
            value={diameter}
            min={30}
            max={600}
            step={1}
            valueDisplay={`${Math.round(diameter)}px`}
            onChange={handleDiameterChange}
            onCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            label={t('widget-editor.thickness', 'Thickness')}
            value={leanVariant.track_thickness}
            min={1}
            max={trackThicknessMax}
            step={1}
            integerDisplay
            valueDisplay={`${leanVariant.track_thickness}px`}
            onSliderChange={(track_thickness) => updateLeanSize({ track_thickness })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.emptyColor', 'Empty Color')}
            value={leanVariant.track_empty_color}
            onChange={(track_empty_color) => updateLean({ track_empty_color })}
          />
          <SliderField
            label={t('widget-editor.emptyOpacity', 'Empty Opacity')}
            value={leanVariant.track_empty_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(leanVariant.track_empty_opacity * 100)}%`}
            onSliderChange={(track_empty_opacity) => updateLean({ track_empty_opacity })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.filledColor', 'Filled Color')}
            value={leanVariant.track_filled_color}
            onChange={(track_filled_color) => updateLean({ track_filled_color })}
          />
          <SliderField
            label={t('widget-editor.filledOpacity', 'Filled Opacity')}
            value={leanVariant.track_filled_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(leanVariant.track_filled_opacity * 100)}%`}
            onSliderChange={(track_filled_opacity) => updateLean({ track_filled_opacity })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.borderColor', 'Border Color')}
            value={leanVariant.track_border_color}
            onChange={(track_border_color) => updateLean({ track_border_color })}
          />
          <SliderField
            label={t('widget-editor.border', 'Border')}
            value={leanVariant.track_border_thickness}
            min={0}
            max={borderThicknessMax}
            step={1}
            integerDisplay
            valueDisplay={`${leanVariant.track_border_thickness}px`}
            onSliderChange={(track_border_thickness) => updateLeanSize({ track_border_thickness })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
      </div>
      <div className="space-y-4">
        <SectionHeading icon={Type} title={t('widget-editor.label', 'Label')} />
        <div className="grid grid-cols-2 gap-4">
          <FontSelectField
            label={t('widget-editor.labelFont', 'Label Font')}
            value={widget.data.font}
            onValueChange={(font) => updateWidgetData(widget.id, { font })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <SizeSlider
            label={t('widget-editor.fontSize', 'Font Size')}
            value={widget.data.font_size}
            min={6}
            max={200}
            valueDisplay={`${widget.data.font_size}px`}
            onChange={(font_size) => updateWidgetSize(widget.id, { font_size })}
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.labelColor', 'Label Color')}
            value={widget.data.color}
            onChange={(color) => updateWidgetData(widget.id, { color })}
          />
          <ColorField
            label={t('widget-editor.unitColor', 'Unit Color')}
            value={widget.data.unit_color}
            onChange={(unit_color) => updateWidgetData(widget.id, { unit_color })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label={t('widget-editor.horizontalOffset', 'Horizontal Offset')}
            value={leanVariant.value_offset_x}
            min={-50}
            max={50}
            step={1}
            integerDisplay
            valueDisplay={`${leanVariant.value_offset_x}px`}
            onSliderChange={(value_offset_x) => updateLeanSize({ value_offset_x })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            label={t('widget-editor.verticalOffset', 'Vertical Offset')}
            value={leanVariant.value_offset_y}
            min={-50}
            max={50}
            step={1}
            integerDisplay
            valueDisplay={`${leanVariant.value_offset_y}px`}
            onSliderChange={(value_offset_y) => updateLeanSize({ value_offset_y })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
      </div>
    </>
  )
}

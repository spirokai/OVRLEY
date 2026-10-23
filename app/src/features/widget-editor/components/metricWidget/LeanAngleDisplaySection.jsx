import { useMemo } from 'react'
import { SlidersHorizontal, Type } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { buildUniformResizeUpdate } from '../../utils/widgetSize'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { ColorField, SizeSlider, SliderField } from '../widgetFormControls'
import TrackStyleSection from './TrackStyleSection'
import FontSelectField from '@/components/ui/font-select-field'
import { useAvailableFonts } from '@/hooks/useFonts'
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
  const updateLean = useDisplayVariantUpdater(widget, 'lean_angle', updateWidgetData)
  const updateLeanSize = useDisplayVariantUpdater(widget, 'lean_angle', updateWidgetSize)

  const diameter = leanVariant.diameter
  const trackThicknessMax = Math.floor((diameter - 1) / 2)
  const borderThicknessMax = Math.min(8, Math.floor((leanVariant.track_thickness - 1) / 2))
  const availableFonts = useAvailableFonts()

  const handleDiameterChange = (nextDiameter) => {
    const update = buildUniformResizeUpdate(widget, nextDiameter)
    updateWidgetSize(widget.id, update)
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
            valueDisplay={`${Math.round(diameter)}`}
            suffix="px"
            onChange={handleDiameterChange}
            onCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            editable
            label={t('widget-editor.thickness', 'Thickness')}
            value={leanVariant.track_thickness}
            min={1}
            max={trackThicknessMax}
            step={1}
            integerDisplay
            valueDisplay={`${leanVariant.track_thickness}`}
            suffix="px"
            onSliderChange={(track_thickness) => updateLeanSize({ track_thickness })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
      </div>
      <TrackStyleSection
        data={leanVariant}
        updateVariant={updateLean}
        updateVariantSize={updateLeanSize}
        onCommit={() => commitWidgetSize(widget.id)}
        borderMax={borderThicknessMax}
      />
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
            labelClassName="text-[0.667rem] text-muted-foreground uppercase font-bold"
          />
          <SizeSlider
            label={t('widget-editor.fontSize', 'Font Size')}
            value={widget.data.font_size}
            min={6}
            max={200}
            valueDisplay={`${widget.data.font_size}`}
            suffix="px"
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
            editable
            label={t('widget-editor.horizontalOffset', 'Horizontal Offset')}
            value={leanVariant.value_offset_x}
            min={-50}
            max={50}
            step={1}
            integerDisplay
            valueDisplay={`${leanVariant.value_offset_x}`}
            suffix="px"
            onSliderChange={(value_offset_x) => updateLeanSize({ value_offset_x })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            editable
            label={t('widget-editor.verticalOffset', 'Vertical Offset')}
            value={leanVariant.value_offset_y}
            min={-50}
            max={50}
            step={1}
            integerDisplay
            valueDisplay={`${leanVariant.value_offset_y}`}
            suffix="px"
            onSliderChange={(value_offset_y) => updateLeanSize({ value_offset_y })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
      </div>
    </>
  )
}

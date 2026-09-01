import { SectionHeading } from '@/components/ui/section-heading'
import { ToggleField, SelectField, SizeSlider, SliderField, ColorField } from '../widgetFormControls'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { useMemo } from 'react'
import { SlidersHorizontal, Tags } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { BarFillStyleDetails, BarFillStyleField } from './BarFillStyleControls'
import { getLinearBarGapMax, getLinearTrackCornerRadiusMax, getSuggestedLinearBarGeometry } from '@/features/widget-preview/shared/gaugeBarGeometry'
import { useTranslation } from 'react-i18next'

const ORIENTATION_OPTIONS = [
  { value: 'horizontal', labelKey: 'widget-editor.horizontal', defaultLabel: 'Horizontal' },
  { value: 'vertical', labelKey: 'widget-editor.vertical', defaultLabel: 'Vertical' },
]

const POSITION_OPTIONS = [
  { value: 'top', labelKey: 'widget-editor.top', defaultLabel: 'Top' },
  { value: 'bottom', labelKey: 'widget-editor.bottom', defaultLabel: 'Bottom' },
  { value: 'left', labelKey: 'widget-editor.left', defaultLabel: 'Left' },
  { value: 'right', labelKey: 'widget-editor.right', defaultLabel: 'Right' },
]

const LABEL_POSITION_SWAP = {
  bottom: 'left',
  top: 'right',
  left: 'bottom',
  right: 'top',
}

/**
 * Renders linear gauge display controls: orientation, track styling,
 * min/max labels.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 */
export default function LinearDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  const linearData = useMemo(() => widget.data.display_variants?.linear ?? {}, [widget.data.display_variants?.linear])
  const updateLinear = useDisplayVariantUpdater(widget, 'linear', linearData, updateWidgetData)
  const updateLinearSize = useDisplayVariantUpdater(widget, 'linear', linearData, updateWidgetSize)
  const availableFonts = useAvailableFonts()
  const orientationOptions = useMemo(
    () => ORIENTATION_OPTIONS.map(({ value, labelKey, defaultLabel }) => ({ value, label: t(labelKey, defaultLabel) })),
    [t],
  )
  const positionOptions = useMemo(() => POSITION_OPTIONS.map(({ value, labelKey, defaultLabel }) => ({ value, label: t(labelKey, defaultLabel) })), [t])
  const cornerRadiusMax = getLinearTrackCornerRadiusMax(linearData)
  const updateOrientation = (orientation) => {
    if (orientation === linearData.orientation) return
    const nextWidth = linearData.height
    const nextHeight = linearData.width
    const nextData = { ...linearData, orientation, width: nextWidth, height: nextHeight }
    updateLinearSize({
      orientation,
      width: nextWidth,
      height: nextHeight,
      track_corner_radius: Math.min(linearData.track_corner_radius, getLinearTrackCornerRadiusMax(nextData)),
      min_max_label_position: LABEL_POSITION_SWAP[linearData.min_max_label_position] ?? linearData.min_max_label_position,
    })
    commitWidgetSize(widget.id)
  }
  const availablePositions = useMemo(() => {
    if (linearData.orientation === 'horizontal') {
      return positionOptions.filter((option) => option.value === 'top' || option.value === 'bottom')
    } else {
      return positionOptions.filter((option) => option.value === 'left' || option.value === 'right')
    }
  }, [linearData.orientation, positionOptions])

  const widthSliderBounds = useMemo(() => {
    if (linearData.orientation === 'vertical') {
      return { min: 8, max: 100 }
    }
    return { min: 20, max: 600 }
  }, [linearData.orientation])

  const heightSliderBounds = useMemo(() => {
    if (linearData.orientation === 'vertical') {
      return { min: 20, max: 600 }
    }
    return { min: 8, max: 100 }
  }, [linearData.orientation])
  const barGapMax = linearData.track_fill_style === 'bars' ? getLinearBarGapMax(linearData) : 0

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={SlidersHorizontal} title={t('widget-editor.gaugeTrack', 'Gauge Track')} />
        <div className="grid grid-cols-2 gap-4">
          <SizeSlider
            label="Width"
            value={linearData.width}
            min={widthSliderBounds.min}
            max={widthSliderBounds.max}
            step={1}
            valueDisplay={`${linearData.width}px`}
            onChange={(value) =>
              updateLinearSize({
                width: value,
                track_corner_radius: Math.min(linearData.track_corner_radius, getLinearTrackCornerRadiusMax({ ...linearData, width: value })),
              })
            }
            onCommit={() => commitWidgetSize(widget.id)}
          />
          <SizeSlider
            label="Height"
            value={linearData.height}
            min={heightSliderBounds.min}
            max={heightSliderBounds.max}
            step={1}
            valueDisplay={`${linearData.height}px`}
            onChange={(value) =>
              updateLinearSize({
                height: value,
                track_corner_radius: Math.min(linearData.track_corner_radius, getLinearTrackCornerRadiusMax({ ...linearData, height: value })),
              })
            }
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4 pt-2">
          <SelectField label={t('widget-editor.orientation', 'Orientation')} value={linearData.orientation} onValueChange={updateOrientation} options={orientationOptions} />
          <BarFillStyleField data={linearData} suggestBarGeometry={getSuggestedLinearBarGeometry} updateVariant={updateLinear} />
          <BarFillStyleDetails
            data={linearData}
            barGapMax={barGapMax}
            getCornerRadiusMax={getLinearTrackCornerRadiusMax}
            updateVariant={updateLinear}
            updateVariantSize={updateLinearSize}
            commitWidgetSize={commitWidgetSize}
            widgetId={widget.id}
          />
          <SliderField
            label={t('widget-editor.cornerRadius', 'Corner Radius')}
            value={linearData.track_corner_radius}
            min={0}
            max={cornerRadiusMax}
            step={1}
            integerDisplay
            valueDisplay={`${linearData.track_corner_radius}px`}
            onSliderChange={(value) => updateLinearSize({ track_corner_radius: Math.min(Math.max(0, value), cornerRadiusMax) })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField label={t('widget-editor.borderColor', 'Border Color')} value={linearData.track_border_color} onChange={(value) => updateLinear({ track_border_color: value })} />
          <SliderField
            label="Border"
            value={linearData.track_border_thickness}
            min={0}
            max={6}
            step={1}
            integerDisplay
            valueDisplay={`${linearData.track_border_thickness}px`}
            onSliderChange={(value) => updateLinearSize({ track_border_thickness: value })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>

        <div className="grid grid-cols-2 gap-4">
          <ColorField label={t('widget-editor.emptyColor', 'Empty Color')} value={linearData.track_empty_color} onChange={(value) => updateLinear({ track_empty_color: value })} />
          <SliderField
            label={t('widget-editor.emptyOpacity', 'Empty Opacity')}
            value={linearData.track_empty_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(linearData.track_empty_opacity * 100)}%`}
            onSliderChange={(value) => updateLinear({ track_empty_opacity: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField label={t('widget-editor.filledColor', 'Filled Color')} value={linearData.track_filled_color} onChange={(value) => updateLinear({ track_filled_color: value })} />
          <SliderField
            label={t('widget-editor.filledOpacity', 'Filled Opacity')}
            value={linearData.track_filled_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round(linearData.track_filled_opacity * 100)}%`}
            onSliderChange={(value) => updateLinear({ track_filled_opacity: value })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <div className="flex flex-row items-center gap-3">
          <SectionHeading icon={Tags} title={t('widget-editor.minmaxLabels', 'Min/Max Labels')} />
          <div className="shrink-0 pt-1">
            <ToggleField checked={linearData.show_min_max_labels} onCheckedChange={(checked) => updateLinear({ show_min_max_labels: checked })} />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4 pt-2">
          <FontSelectField
            label={t('widget-editor.labelFont', 'Label Font')}
            value={linearData.min_max_label_font}
            disabled={!linearData.show_min_max_labels}
            onValueChange={(value) => updateLinear({ min_max_label_font: value })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <SizeSlider
            label={t('widget-editor.fontSize', 'Font Size')}
            disabled={!linearData.show_min_max_labels}
            value={linearData.min_max_label_font_size}
            min={6}
            max={50}
            step={1}
            valueDisplay={`${linearData.min_max_label_font_size}px`}
            onChange={(value) => updateLinearSize({ min_max_label_font_size: value })}
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>

        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.labelColor', 'Label Color')}
            value={linearData.min_max_label_color}
            onChange={(value) => updateLinear({ min_max_label_color: value })}
            disabled={!linearData.show_min_max_labels}
          />
          <SelectField
            label={t('widget-editor.position', 'Position')}
            disabled={!linearData.show_min_max_labels}
            value={linearData.min_max_label_position}
            onValueChange={(value) => updateLinear({ min_max_label_position: value })}
            options={availablePositions}
          />
        </div>
      </div>
    </>
  )
}

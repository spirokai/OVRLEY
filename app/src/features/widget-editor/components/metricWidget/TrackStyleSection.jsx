import { Palette } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import GradientPicker from '@/components/ui/gradient-picker'
import { SectionHeading } from '@/components/ui/section-heading'
import { ColorField, SliderField, ToggleField } from '../widgetFormControls'

/**
 * Track colors, opacities and border of a track-fill display. With `supportsGradient`, the
 * heading switch swaps the solid filled color for a gradient anchored along the track.
 *
 * @param {object} props - Component props.
 * @param {object} props.data - Active display variant data.
 * @param {(update: object) => void} props.updateVariant - Merges a patch into the display variant.
 * @param {(update: object) => void} props.updateVariantSize - Merges a live size patch into the display variant.
 * @param {() => void} props.onCommit - Commits a live size update.
 * @param {number} props.borderMax - Upper bound of the border thickness.
 * @param {boolean} [props.supportsGradient=true] - Whether the display renders gradient fills.
 * @returns {JSX.Element} Rendered component output.
 */
export default function TrackStyleSection({ data, updateVariant, updateVariantSize, onCommit, borderMax, supportsGradient = true }) {
  const { t } = useTranslation()
  const gradient = supportsGradient && data.track_filled_gradient

  return (
    <div className="space-y-4">
      <SectionHeading
        icon={Palette}
        title={t('widget-editor.trackStyle', 'Track Style')}
        trailing={
          supportsGradient ? (
            <div className="flex items-center gap-1.5">
              <span className="text-[0.667rem] text-muted-foreground uppercase font-bold">{t('widget-editor.fillGradient', 'Gradient')}</span>
              <ToggleField
                label={t('widget-editor.fillGradient', 'Gradient')}
                checked={data.track_filled_gradient}
                onCheckedChange={(track_filled_gradient) => updateVariant({ track_filled_gradient })}
              />
            </div>
          ) : null
        }
      />
      <div className="grid grid-cols-2 gap-4">
        <ColorField
          label={t('widget-editor.emptyColor', 'Empty Color')}
          value={data.track_empty_color}
          onChange={(track_empty_color) => updateVariant({ track_empty_color })}
        />
        <SliderField
          editable
          label={t('widget-editor.emptyOpacity', 'Empty Opacity')}
          value={data.track_empty_opacity}
          min={0}
          max={1}
          step={0.05}
          valueScale={100}
          valueDisplay={`${Math.round(data.track_empty_opacity * 100)}`}
          suffix="%"
          onSliderChange={(track_empty_opacity) => updateVariant({ track_empty_opacity })}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <ColorField
          label={t('widget-editor.filledColor', 'Filled Color')}
          value={data.track_filled_color}
          disabled={gradient}
          onChange={(track_filled_color) => updateVariant({ track_filled_color })}
        />
        <SliderField
          editable
          label={t('widget-editor.filledOpacity', 'Filled Opacity')}
          value={data.track_filled_opacity}
          min={0}
          max={1}
          step={0.05}
          valueScale={100}
          valueDisplay={`${Math.round(data.track_filled_opacity * 100)}`}
          suffix="%"
          onSliderChange={(track_filled_opacity) => updateVariant({ track_filled_opacity })}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <ColorField
          label={t('widget-editor.borderColor', 'Border Color')}
          value={data.track_border_color}
          onChange={(track_border_color) => updateVariant({ track_border_color })}
        />
        <SliderField
          editable
          label={t('widget-editor.border', 'Border')}
          value={data.track_border_thickness}
          min={0}
          max={borderMax}
          step={1}
          integerDisplay
          valueDisplay={`${data.track_border_thickness}`}
          suffix="px"
          onSliderChange={(track_border_thickness) => updateVariantSize({ track_border_thickness })}
          onSliderCommit={onCommit}
        />
      </div>
      {gradient ? (
        <GradientPicker
          stops={data.track_filled_gradient_stops}
          onChange={(track_filled_gradient_stops) => updateVariantSize({ track_filled_gradient_stops })}
          onCommit={onCommit}
        />
      ) : null}
    </div>
  )
}

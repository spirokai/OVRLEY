import GradientPicker from '@/components/ui/gradient-picker'
import { Label } from '@/components/ui/label'
import { useTranslation } from 'react-i18next'
import { ColorField, SliderField, ToggleField } from '../widgetFormControls'

/**
 * Filled track color and opacity. With `supportsGradient`, a trailing switch swaps the
 * solid color for a gradient anchored along the track.
 *
 * @param {object} props - Component props.
 * @param {object} props.data - Active display variant data.
 * @param {(update: object) => void} props.updateVariant - Merges a patch into the display variant.
 * @param {boolean} [props.supportsGradient=true] - Whether the display renders gradient fills.
 * @returns {JSX.Element} Rendered component output.
 */
export default function TrackFilledColorFields({ data, updateVariant, supportsGradient = true }) {
  const { t } = useTranslation()
  const gradientLabel = t('widget-editor.fillGradient', 'Gradient')
  const gradient = supportsGradient && data.track_filled_gradient

  return (
    <>
      <div className="grid grid-cols-2 gap-4">
        <ColorField
          label={t('widget-editor.filledColor', 'Filled Color')}
          value={data.track_filled_color}
          disabled={gradient}
          onChange={(track_filled_color) => updateVariant({ track_filled_color })}
          trailing={
            supportsGradient ? (
              <div className="flex shrink-0 items-center gap-1.5">
                <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">{gradientLabel}</Label>
                <ToggleField
                  label={gradientLabel}
                  checked={data.track_filled_gradient}
                  onCheckedChange={(track_filled_gradient) => updateVariant({ track_filled_gradient })}
                />
              </div>
            ) : null
          }
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
      {gradient ? (
        <GradientPicker
          stops={data.track_filled_gradient_stops}
          onChange={(track_filled_gradient_stops) => updateVariant({ track_filled_gradient_stops })}
        />
      ) : null}
    </>
  )
}

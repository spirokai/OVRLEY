/**
 * The current-position marker controls shared by every plot editor.
 */

import { Map } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { translateOptions } from '@/i18n'
import { useTranslation } from 'react-i18next'
import { ColorField, SelectField, SliderField } from './widgetFormControls'

const MARKER_VARIANT_OPTIONS = [
  { value: 'single', labelKey: 'widget-editor.singleCircle', defaultLabel: 'Single Circle' },
  { value: 'ring', labelKey: 'widget-editor.concentricRing', defaultLabel: 'Concentric Ring' },
  { value: 'halo', labelKey: 'widget-editor.solidHalo', defaultLabel: 'Solid Halo' },
]

/**
 * Renders the plot marker section.
 *
 * @param {object} props - Component props.
 * @param {object} props.widget - Plot widget with `marker_*` fields.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetDraft - Applies a live widget draft, published once per frame.
 * @param {Function} props.commitWidgetDraft - Commits the live widget draft.
 * @param {boolean} [props.showColor=true] - Whether the plot has one marker color; charts color markers per series.
 * @param {boolean} [props.disabled=false] - Whether the marker is hidden.
 * @param {React.ReactNode} [props.trailing=null] - Optional heading control.
 * @returns {JSX.Element} Rendered component output.
 */
export default function PlotMarkerSection({
  widget,
  updateWidgetData,
  updateWidgetDraft,
  commitWidgetDraft,
  showColor = true,
  disabled = false,
  trailing = null,
}) {
  const { t } = useTranslation()
  const { marker_variant: variant, marker_size: size, marker_opacity: opacity, marker_variant_diameter: diameter } = widget.data
  const commit = () => commitWidgetDraft(widget.id)

  return (
    <div className="space-y-4">
      <SectionHeading icon={Map} title={t('widget-editor.marker', 'Marker')} trailing={trailing} />
      <div className="grid grid-cols-2 gap-4">
        <SelectField
          label={t('widget-editor.markerType', 'Marker Type')}
          value={variant}
          options={translateOptions(MARKER_VARIANT_OPTIONS, t)}
          disabled={disabled}
          onValueChange={(marker_variant) => updateWidgetData(widget.id, { marker_variant })}
        />
        <SliderField
          editable
          integerDisplay
          label={t('widget-editor.size', 'Size')}
          value={size}
          min={1}
          max={50}
          step={1}
          disabled={disabled}
          valueDisplay={`${size}`}
          suffix="px"
          onSliderChange={(marker_size) => updateWidgetDraft(widget.id, { marker_size })}
          onSliderCommit={commit}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        {showColor ? (
          <ColorField
            disabled={disabled}
            value={widget.data.marker_color}
            onChange={(marker_color) => updateWidgetData(widget.id, { marker_color })}
          />
        ) : null}
        <SliderField
          editable
          label={t('widget-editor.opacity', 'Opacity')}
          value={opacity}
          min={0}
          max={100}
          step={1}
          disabled={disabled}
          valueDisplay={`${opacity}`}
          suffix="%"
          onSliderChange={(marker_opacity) => updateWidgetDraft(widget.id, { marker_opacity })}
          onSliderCommit={commit}
        />
        {variant !== 'single' ? (
          <SliderField
            editable
            integerDisplay
            label={variant === 'ring' ? t('widget-editor.ringDiameter', 'Ring Diameter') : t('widget-editor.haloDiameter', 'Halo Diameter')}
            value={diameter}
            min={Math.max(Math.round(size * 2), 4)}
            max={120}
            step={1}
            disabled={disabled}
            valueDisplay={`${diameter}`}
            suffix="px"
            onSliderChange={(marker_variant_diameter) => updateWidgetDraft(widget.id, { marker_variant_diameter })}
            onSliderCommit={commit}
          />
        ) : null}
      </div>
    </div>
  )
}

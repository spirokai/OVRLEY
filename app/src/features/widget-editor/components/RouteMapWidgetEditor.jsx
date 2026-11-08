/**
 * Supports widget editing flows related to route map widget editor.
 */

import { SliderField } from './widgetFormControls'
import { DimensionsSection } from './widgetEditorSections'
import PlotMarkerSection from './PlotMarkerSection'
import RouteLineSection from './RouteLineSection'
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
export default function RouteMapWidgetEditor({ widget, updateWidgetData, updateWidgetDraft, commitWidgetDraft, setNumericField }) {
  const { t } = useTranslation()
  const rotation = normalizeRotation(widget.data.rotation)
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
        onSliderChange={(rawValue) => updateWidgetDraft(widget.id, { rotation: rawValue })}
        onSliderCommit={() => commitWidgetDraft(widget.id)}
      />

      <RouteLineSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetDraft={updateWidgetDraft}
        commitWidgetDraft={commitWidgetDraft}
        detail={
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
              updateWidgetDraft(widget.id, {
                target_density: Number(value.toFixed(2)),
              })
            }
            onSliderCommit={() => commitWidgetDraft(widget.id)}
          />
        }
      />
      <PlotMarkerSection
        showArrow
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetDraft={updateWidgetDraft}
        commitWidgetDraft={commitWidgetDraft}
      />
    </>
  )
}

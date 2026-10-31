/**
 * The tile map's inspector. Sliders publish through live drafts, once per animation frame.
 */

import { Map as MapIcon, Shapes } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { Label } from '@/components/ui/label'
import { translateOptions } from '@/i18n'
import { useTranslation } from 'react-i18next'
import { mapBuildingsAvailable, mapDiameterUpdate, mapShapeUpdate, maxMapCornerRadius } from '../utils/mapWidget'
import { ColorField, NumberField, SelectField, SliderField, ToggleField } from './widgetFormControls'
import { DimensionsSection } from './widgetEditorSections'
import PlotMarkerSection from './PlotMarkerSection'
import RouteLineSection from './RouteLineSection'

// OpenFreeMap's preset names, which stay as they are in every language.
const MAP_STYLE_OPTIONS = [
  { value: 'liberty', label: 'Liberty' },
  { value: 'bright', label: 'Bright' },
  { value: 'positron', label: 'Positron' },
  { value: 'dark', label: 'Dark' },
  { value: 'fiord', label: 'Fiord' },
]
const CAMERA_MODE_OPTIONS = [
  { value: 'follow', labelKey: 'widget-editor.chaseCamera', defaultLabel: 'Chase Camera' },
  { value: 'fixed', labelKey: 'widget-editor.topDown', defaultLabel: 'Top Down' },
]
const ORIENTATION_OPTIONS = [
  { value: 'rider_up', labelKey: 'widget-editor.riderUp', defaultLabel: 'Rider Up' },
  { value: 'north_up', labelKey: 'widget-editor.northUp', defaultLabel: 'North Up' },
]
const SHAPE_OPTIONS = [
  { value: 'rectangle', labelKey: 'widgets.backdropTypes.rectangle', defaultLabel: 'Rectangle' },
  { value: 'circle', labelKey: 'widgets.backdropTypes.circle', defaultLabel: 'Circle' },
]
function panDisplay(value) {
  return `${Math.round(value * 100)}`
}

function zoomOffsetDisplay(value) {
  return `${value > 0 ? '+' : ''}${value.toFixed(1)}`
}

/**
 * Renders the tile map editor.
 *
 * @param {object} props - Component props.
 * @param {object} props.widget - Map widget being edited.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetDraft - Applies a live widget draft, published once per frame.
 * @param {Function} props.commitWidgetDraft - Commits the live widget draft.
 * @param {Function} props.setNumericField - Sets a numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function MapWidgetEditor({ widget, updateWidgetData, updateWidgetDraft, commitWidgetDraft, setNumericField }) {
  const { t } = useTranslation()
  const { data } = widget
  const isChase = data.camera_mode === 'follow'
  const isCircle = data.shape === 'circle'
  const buildingsAvailable = mapBuildingsAvailable(data)
  const radiusMax = maxMapCornerRadius(data)
  const commit = () => commitWidgetDraft(widget.id)

  return (
    <>
      {isCircle ? (
        <div className="grid grid-cols-2 gap-4">
          <NumberField
            label={t('widget-editor.diameter', 'Diameter')}
            value={data.width}
            onChange={(rawValue) => updateWidgetData(widget.id, mapDiameterUpdate(rawValue, data.width))}
          />
        </div>
      ) : (
        <DimensionsSection widget={widget} setNumericField={setNumericField} min={8} />
      )}

      <div className="space-y-4">
        <SectionHeading icon={MapIcon} title={t('widget-editor.mapView', 'Map View')} />
        <div className="grid grid-cols-2 gap-4">
          <SelectField
            label={t('widget-editor.mapStyle', 'Map Style')}
            value={data.map_style}
            options={MAP_STYLE_OPTIONS}
            onValueChange={(map_style) => updateWidgetData(widget.id, { map_style })}
          />
          <SelectField
            label={t('widget-editor.cameraMode', 'Camera Mode')}
            value={data.camera_mode}
            options={translateOptions(CAMERA_MODE_OPTIONS, t)}
            onValueChange={(camera_mode) => updateWidgetData(widget.id, { camera_mode })}
          />
        </div>
        {isChase ? (
          <>
            <SelectField
              label={t('widget-editor.orientation', 'Orientation')}
              value={data.orientation}
              options={translateOptions(ORIENTATION_OPTIONS, t)}
              onValueChange={(orientation) => updateWidgetData(widget.id, { orientation })}
            />
            <div className="grid grid-cols-2 gap-4">
              <SliderField
                editable
                label={t('widget-editor.zoom', 'Zoom')}
                value={data.follow_zoom}
                min={14}
                max={19}
                step={0.1}
                valueDisplay={data.follow_zoom.toFixed(1)}
                onSliderChange={(value) => updateWidgetDraft(widget.id, { follow_zoom: Number(value.toFixed(1)) })}
                onSliderCommit={commit}
              />
              <SliderField
                editable
                integerDisplay
                label={t('widget-editor.pitch', 'Pitch')}
                value={data.pitch}
                min={0}
                max={70}
                step={1}
                suffix="°"
                onSliderChange={(pitch) => updateWidgetDraft(widget.id, { pitch })}
                onSliderCommit={commit}
              />
            </div>
          </>
        ) : (
          <>
            <SliderField
              editable
              label={t('widget-editor.zoom', 'Zoom')}
              value={data.fixed_zoom}
              min={-4}
              max={8}
              step={0.1}
              valueDisplay={zoomOffsetDisplay(data.fixed_zoom)}
              onSliderChange={(value) => updateWidgetDraft(widget.id, { fixed_zoom: Number(value.toFixed(1)) })}
              onSliderCommit={commit}
            />
            <div className="grid grid-cols-2 gap-4">
              <SliderField
                editable
                label={t('widget-editor.routeOffsetX', 'Route Offset X')}
                value={data.fixed_pan_x}
                min={-1}
                max={1}
                step={0.005}
                valueScale={100}
                valueDisplay={panDisplay(data.fixed_pan_x)}
                suffix="%"
                onSliderChange={(fixed_pan_x) => updateWidgetDraft(widget.id, { fixed_pan_x })}
                onSliderCommit={commit}
              />
              <SliderField
                editable
                label={t('widget-editor.routeOffsetY', 'Route Offset Y')}
                value={data.fixed_pan_y}
                min={-1}
                max={1}
                step={0.005}
                valueScale={100}
                valueDisplay={panDisplay(data.fixed_pan_y)}
                suffix="%"
                onSliderChange={(fixed_pan_y) => updateWidgetDraft(widget.id, { fixed_pan_y })}
                onSliderCommit={commit}
              />
            </div>
          </>
        )}
        <div className="flex items-center justify-between gap-2 pl-1">
          <Label className="p-0 text-[0.667rem] text-muted-foreground uppercase font-bold" disabled={!buildingsAvailable}>
            {t('widget-editor.mapBuildings', 'Buildings')}
          </Label>
          <ToggleField
            checked={buildingsAvailable && data.show_buildings}
            disabled={!buildingsAvailable}
            onCheckedChange={(show_buildings) => updateWidgetData(widget.id, { show_buildings })}
          />
        </div>
        <SelectField
          label={t('widget-editor.msaa', 'Anti-aliasing (MSAA)')}
          value={String(data.msaa_samples)}
          options={[
            { value: '4', label: '4x' },
            { value: '1', label: t('widget-editor.msaaOff', 'Off') },
          ]}
          onValueChange={(value) => updateWidgetData(widget.id, { msaa_samples: Number(value) })}
        />
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Shapes} title={t('widget-editor.shape', 'Shape')} />
        <div className="grid grid-cols-2 gap-4">
          <SelectField
            label={t('widget-editor.shape', 'Shape')}
            value={data.shape}
            options={translateOptions(SHAPE_OPTIONS, t)}
            onValueChange={(shape) => updateWidgetData(widget.id, mapShapeUpdate(data, shape))}
          />
          {isCircle ? null : (
            <SliderField
              editable
              integerDisplay
              label={t('widget-editor.cornerRadius', 'Corner Radius')}
              value={Math.min(data.corner_radius, radiusMax)}
              min={0}
              max={radiusMax}
              step={1}
              suffix="px"
              onSliderChange={(corner_radius) => updateWidgetDraft(widget.id, { corner_radius })}
              onSliderCommit={commit}
            />
          )}
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.borderColor', 'Border Color')}
            value={data.border_color}
            onChange={(border_color) => updateWidgetData(widget.id, { border_color })}
          />
          <SliderField
            editable
            integerDisplay
            label={t('widget-editor.borderThickness', 'Border Thickness')}
            value={data.border_thickness}
            min={0}
            max={50}
            step={1}
            suffix="px"
            onSliderChange={(border_thickness) => updateWidgetDraft(widget.id, { border_thickness })}
            onSliderCommit={commit}
          />
        </div>
        <SliderField
          editable
          label={t('widget-editor.borderOpacity', 'Border Opacity')}
          value={data.border_opacity}
          min={0}
          max={1}
          step={0.05}
          valueScale={100}
          valueDisplay={`${Math.round(data.border_opacity * 100)}`}
          suffix="%"
          onSliderChange={(border_opacity) => updateWidgetDraft(widget.id, { border_opacity })}
          onSliderCommit={commit}
        />
      </div>

      <RouteLineSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetDraft={updateWidgetDraft}
        commitWidgetDraft={commitWidgetDraft}
        separateThickness
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

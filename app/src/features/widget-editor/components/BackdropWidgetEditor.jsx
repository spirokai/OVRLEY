import { Palette, Shapes } from 'lucide-react'
import { useCallback, useMemo } from 'react'
import { getBackdropTypeOptions } from '@/lib/widget/standard-widgets'
import { initBackdropVariant } from '@/lib/widget/widget-resolver'
import { SectionHeading } from '@/components/ui/section-heading'
import { parseInteger } from '../utils/widgetUtils'
import { ColorField, NumberField, SelectField, SliderField } from './widgetFormControls'
import { useTranslation } from 'react-i18next'

const RECTANGLE_CORNERS = [
  { key: 'round_top_left', labelKey: 'widget-editor.roundTopLeftCorner', defaultLabel: 'Round top left corner', className: 'rounded-tl-md' },
  { key: 'round_top_right', labelKey: 'widget-editor.roundTopRightCorner', defaultLabel: 'Round top right corner', className: 'rounded-tr-md' },
  { key: 'round_bottom_left', labelKey: 'widget-editor.roundBottomLeftCorner', defaultLabel: 'Round bottom left corner', className: 'rounded-bl-md' },
  {
    key: 'round_bottom_right',
    labelKey: 'widget-editor.roundBottomRightCorner',
    defaultLabel: 'Round bottom right corner',
    className: 'rounded-br-md',
  },
]

function maxBorderThickness(data) {
  if (data.display_type === 'circle') {
    return Math.max(0, Math.floor(((data.diameter ?? 1) - 1) * 0.2))
  }
  return Math.max(0, Math.floor((Math.min(data.width ?? 1, data.height ?? 1) - 1) * 0.2))
}

function CornerGrid({ rectangleData, onToggle }) {
  const { t } = useTranslation()
  return (
    <div
      className="grid h-32 w-32 grid-cols-2 grid-rows-2 gap-2 py-3 pr-3 pl-1"
      aria-label={t('widget-editor.roundedCorners', 'Rounded corners')}
      data-testid="corner-grid"
    >
      {RECTANGLE_CORNERS.map((corner) => {
        const active = Boolean(rectangleData[corner.key])
        return (
          <button
            key={corner.key}
            type="button"
            aria-label={t(corner.labelKey, corner.defaultLabel)}
            aria-pressed={active}
            onClick={() => onToggle(corner.key, !active)}
            className={`relative min-h-0 min-w-0 border transition-colors cursor-pointer ${corner.className} ${
              active ? 'border-primary bg-primary/30' : 'border-border/70 bg-background hover:bg-surface-accent-soft '
            }`}
          >
            <span
              className={`absolute h-5 w-5 border-primary ${corner.className} ${
                corner.key.includes('top') ? 'top-2' : 'bottom-2'
              } ${corner.key.includes('left') ? 'left-2' : 'right-2'} ${active ? 'border-2' : 'border border-dashed opacity-50'}`}
            />
          </button>
        )
      })}
    </div>
  )
}

export default function BackdropWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  const displayType = widget.data.display_type || 'rectangle'
  const displayOptions = useMemo(() => getBackdropTypeOptions(t), [t])
  const activeData = useMemo(() => widget.data.display_variants?.[displayType] ?? {}, [displayType, widget.data.display_variants])
  const resolvedData = { ...widget.data, ...activeData, display_type: displayType }
  const borderMax = maxBorderThickness(resolvedData)

  const buildActiveVariantUpdate = useCallback(
    (updates) => ({
      display_variants: {
        ...(widget.data.display_variants || {}),
        [displayType]: {
          ...activeData,
          ...updates,
        },
      },
    }),
    [activeData, displayType, widget.data.display_variants],
  )

  const updateActiveVariant = useCallback(
    (updates) => {
      updateWidgetData(widget.id, buildActiveVariantUpdate(updates))
    },
    [buildActiveVariantUpdate, updateWidgetData, widget.id],
  )

  const updateActiveVariantSize = useCallback(
    (updates) => {
      updateWidgetSize(widget.id, buildActiveVariantUpdate(updates))
    },
    [buildActiveVariantUpdate, updateWidgetSize, widget.id],
  )

  const handleDisplayTypeChange = useCallback(
    (value) => {
      const nextData = initBackdropVariant(widget.data, value)
      updateWidgetData(widget.id, { display_type: value, display_variants: nextData.display_variants })
    },
    [updateWidgetData, widget.data, widget.id],
  )

  const renderSizeControls = () => {
    if (displayType === 'circle') {
      return (
        <div className="grid grid-cols-2 gap-4">
          <NumberField
            label={t('widget-editor.diameter', 'Diameter')}
            value={activeData.diameter}
            min={1}
            onChange={(rawValue) => updateActiveVariant({ diameter: parseInteger(rawValue, activeData.diameter ?? 1) })}
          />
        </div>
      )
    }

    return (
      <div className="grid grid-cols-2 gap-4">
        <NumberField
          label={t('widget-editor.width', 'Width')}
          value={activeData.width}
          min={1}
          onChange={(rawValue) => updateRectangleSize({ width: parseInteger(rawValue, activeData.width ?? 1) })}
        />
        <NumberField
          label={t('widget-editor.height', 'Height')}
          value={activeData.height}
          min={1}
          onChange={(rawValue) => updateRectangleSize({ height: parseInteger(rawValue, activeData.height ?? 1) })}
        />
      </div>
    )
  }

  const updateRectangleSize = useCallback(
    (updates) => {
      const nextWidth = updates.width ?? activeData.width
      const nextHeight = updates.height ?? activeData.height
      const cornerRadiusMax = Math.max(0, Math.min(nextWidth ?? 0, nextHeight ?? 0) * 0.5)
      updateActiveVariant({
        ...updates,
        corner_radius: Math.min(Math.max(0, activeData.corner_radius ?? 0), cornerRadiusMax),
      })
    },
    [activeData.corner_radius, activeData.height, activeData.width, updateActiveVariant],
  )

  const renderShapeControls = () => {
    const radiusMax = Math.max(0, Math.min(activeData.width ?? 0, activeData.height ?? 0) * 0.5)
    const cornerRadius = Math.min(Math.max(0, activeData.corner_radius ?? 0), radiusMax)

    return (
      <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-4">
        <CornerGrid
          rectangleData={activeData}
          onToggle={(key, checked) => {
            updateActiveVariant({ [key]: checked })
          }}
        />
        <div className="grid h-32 grid-rows-2" data-testid="corner-control-rows">
          <div className="min-h-0">
            <SliderField
              label={t('widget-editor.borderThickness', 'Border Thickness')}
              value={Math.min(widget.data.border_thickness ?? 0, borderMax)}
              min={0}
              max={borderMax}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.border_thickness ?? 0}px`}
              onSliderChange={(value) => updateWidgetSize(widget.id, { border_thickness: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          </div>
          <div className="min-h-0">
            <SliderField
              label={t('widget-editor.cornerRadius', 'Corner Radius')}
              value={cornerRadius}
              min={0}
              max={radiusMax}
              step={1}
              integerDisplay
              valueDisplay={`${cornerRadius}px`}
              onSliderChange={(value) => updateActiveVariantSize({ corner_radius: Math.min(Math.max(0, value), radiusMax) })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          </div>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="space-y-4">
        <SelectField
          label={t('widget-editor.displayType', 'Display Type')}
          value={displayType}
          onValueChange={handleDisplayTypeChange}
          options={displayOptions}
        />
        {renderSizeControls()}
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Palette} title={t('widget-editor.backdropStyle', 'Backdrop Style')} />
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.fillColor', 'Fill Color')}
            value={widget.data.fill_color}
            onChange={(value) => updateWidgetData(widget.id, { fill_color: value })}
          />
          <ColorField
            label={t('widget-editor.borderColor', 'Border Color')}
            value={widget.data.border_color}
            onChange={(value) => updateWidgetData(widget.id, { border_color: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label={t('widget-editor.fillOpacity', 'Fill Opacity')}
            value={widget.data.fill_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round((widget.data.fill_opacity ?? 0) * 100)}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { fill_opacity: value })}
          />
          <SliderField
            label={t('widget-editor.borderOpacity', 'Border Opacity')}
            value={widget.data.border_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round((widget.data.border_opacity ?? 0) * 100)}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { border_opacity: value })}
          />
        </div>
        {displayType === 'circle' ? (
          <SliderField
            label={t('widget-editor.borderThickness', 'Border Thickness')}
            value={Math.min(widget.data.border_thickness ?? 0, borderMax)}
            min={0}
            max={borderMax}
            step={1}
            integerDisplay
            valueDisplay={`${widget.data.border_thickness ?? 0}px`}
            onSliderChange={(value) => updateWidgetSize(widget.id, { border_thickness: value })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        ) : null}
      </div>
      {displayType === 'rectangle' ? (
        <div className="space-y-4">
          <SectionHeading icon={Shapes} title={t('widget-editor.shape', 'Shape')} />
          {renderShapeControls()}
        </div>
      ) : null}
    </>
  )
}

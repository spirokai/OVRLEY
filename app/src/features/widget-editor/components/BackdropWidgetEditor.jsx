import { getWidgetDisplayTypeOptions } from '@/lib/widget/widget-catalog'
import { Palette, Shapes } from 'lucide-react'
import { useCallback, useMemo } from 'react'
import { initDisplayVariant } from '@/lib/widget/widget-resolver'
import { SectionHeading } from '@/components/ui/section-heading'
import { parseInteger } from '../utils/widgetInput'
import { maxEdgeStripThickness, toggleEdgeStrip } from '../utils/backdropShape'
import { ColorField, NumberField, SelectField, SliderField } from './widgetFormControls'
import { useTranslation } from 'react-i18next'

const RECTANGLE_CORNERS = {
  round_top_left: { labelKey: 'widget-editor.roundTopLeftCorner', defaultLabel: 'Round top left corner', className: 'rounded-tl-xl' },
  round_top_right: { labelKey: 'widget-editor.roundTopRightCorner', defaultLabel: 'Round top right corner', className: 'rounded-tr-xl' },
  round_bottom_left: { labelKey: 'widget-editor.roundBottomLeftCorner', defaultLabel: 'Round bottom left corner', className: 'rounded-bl-xl' },
  round_bottom_right: { labelKey: 'widget-editor.roundBottomRightCorner', defaultLabel: 'Round bottom right corner', className: 'rounded-br-xl' },
}

const RECTANGLE_EDGES = {
  top: { labelKey: 'widget-editor.topEdgeStrip', defaultLabel: 'Top edge strip', className: 'top-1 inset-x-1 h-1' },
  right: { labelKey: 'widget-editor.rightEdgeStrip', defaultLabel: 'Right edge strip', className: 'right-1 inset-y-1 w-1' },
  bottom: { labelKey: 'widget-editor.bottomEdgeStrip', defaultLabel: 'Bottom edge strip', className: 'bottom-1 inset-x-1 h-1' },
  left: { labelKey: 'widget-editor.leftEdgeStrip', defaultLabel: 'Left edge strip', className: 'left-1 inset-y-1 w-1' },
}

// Spatial 3x3 layout: corners at the corners, edges between them, the backdrop body in the middle.
const SHAPE_GRID_CELLS = [
  { kind: 'corner', key: 'round_top_left' },
  { kind: 'edge', key: 'top' },
  { kind: 'corner', key: 'round_top_right' },
  { kind: 'edge', key: 'left' },
  { kind: 'center', key: 'center' },
  { kind: 'edge', key: 'right' },
  { kind: 'corner', key: 'round_bottom_left' },
  { kind: 'edge', key: 'bottom' },
  { kind: 'corner', key: 'round_bottom_right' },
]

function maxBorderThickness(data) {
  if (data.display_type === 'circle') {
    return Math.max(0, Math.floor(((data.diameter ?? 1) - 1) * 0.2))
  }
  return Math.max(0, Math.floor((Math.min(data.width ?? 1, data.height ?? 1) - 1) * 0.2))
}

function ShapeToggleButton({ label, active, onClick, className, indicatorClassName = null }) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active}
      onClick={onClick}
      className={`relative min-h-0 min-w-0 border transition-colors cursor-pointer ${className} ${
        active ? 'border-primary bg-primary/30' : 'border-border/70 bg-background hover:bg-surface-accent-soft'
      }`}
    >
      {indicatorClassName ? <span className={`absolute ${indicatorClassName}`} /> : null}
    </button>
  )
}

function ShapeGrid({ rectangleData, onToggleCorner, onToggleEdge }) {
  const { t } = useTranslation()
  return (
    <div
      className="grid h-32 w-32 grid-cols-3 grid-rows-3 gap-1.5 py-3 pr-3 pl-1"
      aria-label={t('widget-editor.cornersAndEdges', 'Corners & edges')}
      data-testid="corner-grid"
    >
      {SHAPE_GRID_CELLS.map((cell) => {
        if (cell.kind === 'center') return <div key={cell.key} aria-hidden className="rounded-sm border border-dashed border-border/70" />
        if (cell.kind === 'corner') {
          const corner = RECTANGLE_CORNERS[cell.key]
          const active = rectangleData[cell.key]
          return (
            <ShapeToggleButton
              key={cell.key}
              label={t(corner.labelKey, corner.defaultLabel)}
              active={active}
              onClick={() => onToggleCorner(cell.key, !active)}
              className={`rounded-sm ${corner.className}`}
            />
          )
        }
        const edge = RECTANGLE_EDGES[cell.key]
        const active = rectangleData.edge_strips.includes(cell.key)
        return (
          <ShapeToggleButton
            key={cell.key}
            label={t(edge.labelKey, edge.defaultLabel)}
            active={active}
            onClick={() => onToggleEdge(cell.key)}
            className="rounded-sm"
            indicatorClassName={`rounded-full ${edge.className} ${active ? 'bg-primary' : 'border border-dashed border-primary opacity-50'}`}
          />
        )
      })}
    </div>
  )
}

export default function BackdropWidgetEditor({ widget, updateWidgetData, updateWidgetDraft, commitWidgetDraft }) {
  const { t } = useTranslation()
  const displayType = widget.data.display_type
  const displayOptions = useMemo(() => getWidgetDisplayTypeOptions('backdrop', t), [t])
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

  const updateActiveVariantDraft = useCallback(
    (updates) => {
      updateWidgetDraft(widget.id, buildActiveVariantUpdate(updates))
    },
    [buildActiveVariantUpdate, updateWidgetDraft, widget.id],
  )

  const handleDisplayTypeChange = useCallback(
    (value) => {
      const nextData = initDisplayVariant(widget.data, value)
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
        edge_strip_thickness: Math.min(activeData.edge_strip_thickness, maxEdgeStripThickness(nextWidth, nextHeight)),
      })
    },
    [activeData.corner_radius, activeData.edge_strip_thickness, activeData.height, activeData.width, updateActiveVariant],
  )

  const renderShapeControls = () => {
    const radiusMax = Math.max(0, Math.min(activeData.width ?? 0, activeData.height ?? 0) * 0.5)
    const cornerRadius = Math.min(Math.max(0, activeData.corner_radius ?? 0), radiusMax)
    const stripMax = maxEdgeStripThickness(activeData.width, activeData.height)

    return (
      <>
        <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-4">
          <ShapeGrid
            rectangleData={activeData}
            onToggleCorner={(key, checked) => {
              updateActiveVariant({ [key]: checked })
            }}
            onToggleEdge={(edge) => {
              updateActiveVariant({ edge_strips: toggleEdgeStrip(activeData.edge_strips, edge) })
            }}
          />
          <div className="grid h-32 grid-rows-2" data-testid="corner-control-rows">
            <div className="min-h-0">
              <SliderField
                editable
                label={t('widget-editor.borderThickness', 'Border Thickness')}
                value={Math.min(widget.data.border_thickness ?? 0, borderMax)}
                min={0}
                max={borderMax}
                step={1}
                integerDisplay
                valueDisplay={`${widget.data.border_thickness ?? 0}`}
                suffix="px"
                onSliderChange={(value) => updateWidgetDraft(widget.id, { border_thickness: value })}
                onSliderCommit={() => commitWidgetDraft(widget.id)}
              />
            </div>
            <div className="min-h-0">
              <SliderField
                editable
                label={t('widget-editor.cornerRadius', 'Corner Radius')}
                value={cornerRadius}
                min={0}
                max={radiusMax}
                step={1}
                integerDisplay
                valueDisplay={`${cornerRadius}`}
                suffix="px"
                onSliderChange={(value) => updateActiveVariantDraft({ corner_radius: Math.min(Math.max(0, value), radiusMax) })}
                onSliderCommit={() => commitWidgetDraft(widget.id)}
              />
            </div>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.stripThickness', 'Strip Thickness')}
            value={activeData.edge_strip_thickness}
            min={0}
            max={stripMax}
            step={1}
            integerDisplay
            valueDisplay={`${activeData.edge_strip_thickness}`}
            suffix="px"
            onSliderChange={(value) => updateActiveVariantDraft({ edge_strip_thickness: Math.min(value, stripMax) })}
            onSliderCommit={() => commitWidgetDraft(widget.id)}
          />
          <ColorField
            label={t('widget-editor.stripColor', 'Strip Color')}
            value={activeData.edge_strip_color}
            onChange={(value) => updateActiveVariant({ edge_strip_color: value })}
          />
        </div>
      </>
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
            editable
            label={t('widget-editor.fillOpacity', 'Fill Opacity')}
            value={widget.data.fill_opacity}
            min={0}
            max={1}
            step={0.05}
            valueScale={100}
            valueDisplay={`${Math.round((widget.data.fill_opacity ?? 0) * 100)}`}
            suffix="%"
            onSliderChange={(value) => updateWidgetDraft(widget.id, { fill_opacity: value })}
            onSliderCommit={() => commitWidgetDraft(widget.id)}
          />
          <SliderField
            editable
            label={t('widget-editor.borderOpacity', 'Border Opacity')}
            value={widget.data.border_opacity}
            min={0}
            max={1}
            step={0.05}
            valueScale={100}
            valueDisplay={`${Math.round((widget.data.border_opacity ?? 0) * 100)}`}
            suffix="%"
            onSliderChange={(value) => updateWidgetDraft(widget.id, { border_opacity: value })}
            onSliderCommit={() => commitWidgetDraft(widget.id)}
          />
        </div>
        {displayType === 'circle' ? (
          <SliderField
            editable
            label={t('widget-editor.borderThickness', 'Border Thickness')}
            value={Math.min(widget.data.border_thickness ?? 0, borderMax)}
            min={0}
            max={borderMax}
            step={1}
            integerDisplay
            valueDisplay={`${widget.data.border_thickness ?? 0}`}
            suffix="px"
            onSliderChange={(value) => updateWidgetDraft(widget.id, { border_thickness: value })}
            onSliderCommit={() => commitWidgetDraft(widget.id)}
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

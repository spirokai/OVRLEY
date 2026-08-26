import { CircleGauge } from 'lucide-react'
import { Label } from '@/components/ui/label'
import { SectionHeading } from '@/components/ui/section-heading'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import useDisplayVariantUpdater from '../hooks/useDisplayVariantUpdater'
import GForceDisplaySection from './metricWidget/GForceDisplaySection'

const AXES = [
  { value: 'x', label: 'X' },
  { value: 'y', label: 'Y' },
  { value: 'z', label: 'Z' },
]

/**
 * Renders the axis selector for one G-force direction.
 *
 * @param {object} props
 * @param {string} props.label - Direction label shown next to the selector.
 * @param {string} props.value - Selected axis value.
 * @param {Function} props.onValueChange - Persists an axis selection.
 * @param {Function} props.onInvertChange - Persists the invert setting.
 * @param {boolean} props.inverted - Whether the selected axis is inverted.
 * @param {string} props.switchId - Stable DOM id for the invert switch.
 * @returns {JSX.Element}
 */
function GForceAxisRow({ label, value, onValueChange, onInvertChange, inverted, switchId }) {
  return (
    <div className="space-y-2" data-testid={`${label.toLowerCase()}-axis-row`}>
      <Label className="text-[9px] text-muted-foreground uppercase font-bold">{label} axis</Label>
      <div className="grid grid-cols-2 gap-4">
        <Tabs value={value} onValueChange={onValueChange}>
          <TabsList variant="toolbar" aria-label={`${label} axis`}>
            {AXES.map((axis) => (
              <span key={axis.value} className="inline-flex">
                <TabsTrigger value={axis.value} variant="toolbar" className="p-3 px-5 font-bold">
                  {axis.label}
                </TabsTrigger>
              </span>
            ))}
          </TabsList>
        </Tabs>
        <div className="flex justify-between items-center gap-2 pl-1">
          <Label htmlFor={switchId} className="text-[9px] text-muted-foreground uppercase font-bold">
            Invert sign
          </Label>
          <Switch id={switchId} checked={inverted} onCheckedChange={onInvertChange} />
        </div>
      </div>
    </div>
  )
}

/** Renders G-force display controls and axis mapping controls. */
export default function GForceWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const data = widget.data.display_variants.g_force
  const updateGForce = useDisplayVariantUpdater(widget, 'g_force', data, updateWidgetData)
  const selectHorizontalAxis = (axis_horizontal) => {
    updateGForce(axis_horizontal === data.axis_vertical ? { axis_horizontal, axis_vertical: data.axis_horizontal } : { axis_horizontal })
  }
  const selectVerticalAxis = (axis_vertical) => {
    updateGForce(axis_vertical === data.axis_horizontal ? { axis_horizontal: data.axis_vertical, axis_vertical } : { axis_vertical })
  }

  return (
    <>
      <GForceDisplaySection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
      />
      <div className="space-y-4">
        <SectionHeading icon={CircleGauge} title="Axis Mapping" />
        <div className="grid grid-cols-1 gap-4">
          <GForceAxisRow
            label="Horizontal"
            value={data.axis_horizontal}
            onValueChange={selectHorizontalAxis}
            inverted={data.invert_horizontal}
            onInvertChange={(invert_horizontal) => updateGForce({ invert_horizontal })}
            switchId={`${widget.id}-invert-horizontal`}
          />
          <GForceAxisRow
            label="Vertical"
            value={data.axis_vertical}
            onValueChange={selectVerticalAxis}
            inverted={data.invert_vertical}
            onInvertChange={(invert_vertical) => updateGForce({ invert_vertical })}
            switchId={`${widget.id}-invert-vertical`}
          />
        </div>
      </div>
    </>
  )
}

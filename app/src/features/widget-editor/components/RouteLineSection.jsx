/**
 * The route line controls shared by the route map and the tile map.
 */

import { Palette } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { getThemeColor } from '@/lib/color-utils'
import { Label } from '@/components/ui/label'
import { useTranslation } from 'react-i18next'
import { ColorField, SliderField, ToggleField } from './widgetFormControls'

/**
 * @param {object} props - Component props.
 * @param {object} props.widget - Widget with the route line fields.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetDraft - Applies a live widget draft, published once per frame.
 * @param {Function} props.commitWidgetDraft - Commits the live widget draft.
 * @param {React.ReactNode} [props.thickness=null] - Replaces the thickness slider for a widget that stores its own width.
 * @param {React.ReactNode} [props.detail=null] - Control beside smoothing.
 * @param {React.ReactNode} [props.smoothing=null] - Replaces the simplification slider for a widget that smooths its own way.
 * @returns {JSX.Element} Rendered component output.
 */
export default function RouteLineSection({
  widget,
  updateWidgetData,
  updateWidgetDraft,
  commitWidgetDraft,
  thickness = null,
  detail = null,
  smoothing = null,
}) {
  const { t } = useTranslation()
  const lineWidth = widget.data.completed_line_width ?? widget.data.remaining_line_width
  const completedLineOpacity = widget.data.completed_line_opacity
  const remainingLineOpacity = widget.data.remaining_line_opacity
  const commit = () => commitWidgetDraft(widget.id)

  return (
    <div className="space-y-4">
      <SectionHeading icon={Palette} title={t('widget-editor.lineStyling', 'Line Styling')} />
      {thickness ?? (
        <SliderField
          editable
          label={t('widget-editor.thickness', 'Thickness')}
          value={lineWidth}
          min={0}
          max={20}
          step={1}
          integerDisplay
          valueDisplay={`${lineWidth}`}
          suffix="px"
          onSliderChange={(value) =>
            updateWidgetDraft(widget.id, {
              completed_line_width: value,
              remaining_line_width: value,
            })
          }
          onSliderCommit={commit}
        />
      )}
      <div className="grid grid-cols-2 gap-4">
        {smoothing ?? (
          <SliderField
            editable
            label={t('widget-editor.smoothing', 'Smoothing')}
            value={widget.data.simplify_tolerance_px}
            min={0}
            max={4}
            step={0.05}
            valueDisplay={`${widget.data.simplify_tolerance_px.toFixed(2)}`}
            suffix="px"
            onSliderChange={(value) =>
              updateWidgetDraft(widget.id, {
                simplify_tolerance_px: Number(value.toFixed(2)),
              })
            }
            onSliderCommit={commit}
          />
        )}
        {detail}
      </div>
      <div className="grid grid-cols-2 gap-4">
        <ColorField
          label={t('widget-editor.finishedColor', 'Finished Color')}
          value={widget.data.completed_line_color || getThemeColor('ice')}
          onChange={(value) =>
            updateWidgetData(widget.id, {
              completed_line_color: value,
              color: value,
            })
          }
        />
        <ColorField
          label={t('widget-editor.remainingColor', 'Remaining Color')}
          value={widget.data.remaining_line_color || getThemeColor('teal')}
          onChange={(value) => updateWidgetData(widget.id, { remaining_line_color: value })}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <SliderField
          editable
          label={t('widget-editor.finishedOpacity', 'Finished Opacity')}
          value={completedLineOpacity}
          min={0}
          max={100}
          step={1}
          valueDisplay={`${completedLineOpacity}`}
          suffix="%"
          onSliderChange={(value) => updateWidgetDraft(widget.id, { completed_line_opacity: value })}
          onSliderCommit={commit}
        />
        <SliderField
          editable
          label={t('widget-editor.remainingOpacity', 'Remaining Opacity')}
          value={remainingLineOpacity}
          min={0}
          max={100}
          step={1}
          valueDisplay={`${remainingLineOpacity}`}
          suffix="%"
          onSliderChange={(value) => updateWidgetDraft(widget.id, { remaining_line_opacity: value })}
          onSliderCommit={commit}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <div className="flex items-center justify-between gap-2 pl-1 pt-2 pb-2">
          <Label className="p-0 text-[0.667rem] text-muted-foreground uppercase font-bold">
            {t('widget-editor.showFullActivity', 'Show Full Activity')}
          </Label>
          <ToggleField
            checked={widget.data.show_full_activity}
            onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_activity: checked })}
          />
        </div>
      </div>
    </div>
  )
}

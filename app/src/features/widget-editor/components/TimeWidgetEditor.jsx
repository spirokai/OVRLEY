/**
 * Supports widget editing flows related to time widget editor.
 */

import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Label } from '@/components/ui/label'
import { ELAPSED_TIME_ORIGINS, TIME_WIDGET_MODES } from '@/lib/widget/standard-widgets'
import { useTranslation } from 'react-i18next'
import { FieldBlock, SelectField, TIME_FORMATS, ToggleField } from './widgetFormControls'
import { translateOptions } from '@/i18n'
import { FontSection, IconSection } from './widgetEditorSections'

/**
 * Renders the time widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function TimeWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize, setNumericField }) {
  const { t } = useTranslation()
  const isElapsed = widget.data.time_mode === 'elapsed'

  return (
    <>
      <div className="space-y-4">
        <FieldBlock label={t('widget-editor.timeMode', 'Time Mode')}>
          <ToggleGroup
            type="single"
            className="w-full"
            value={widget.data.time_mode}
            onValueChange={(timeMode) => {
              if (timeMode) updateWidgetData(widget.id, { time_mode: timeMode })
            }}
          >
            {TIME_WIDGET_MODES.map((mode) => (
              <ToggleGroupItem key={mode.value} value={mode.value} className="h-7 w-full px-3 text-xs">
                {t(mode.labelKey, mode.defaultLabel)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </FieldBlock>

        {isElapsed ? (
          <>
            <FieldBlock label={t('widget-editor.elapsedFrom', 'Elapsed From')}>
              <ToggleGroup
                type="single"
                className="w-full"
                value={widget.data.elapsed_origin}
                onValueChange={(elapsedOrigin) => {
                  if (elapsedOrigin) updateWidgetData(widget.id, { elapsed_origin: elapsedOrigin })
                }}
              >
                {ELAPSED_TIME_ORIGINS.map((origin) => (
                  <ToggleGroupItem key={origin.value} value={origin.value} className="h-7 w-full px-3 text-xs">
                    {t(origin.labelKey, origin.defaultLabel)}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </FieldBlock>

            <div className="grid grid-cols-2 gap-4">
              <div className="flex items-center justify-between gap-2 pl-1 pt-2 pb-2">
                <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.hundredths', 'Hundredths')}</Label>
                <ToggleField
                  checked={widget.data.show_hundredths}
                  onCheckedChange={(showHundredths) => updateWidgetData(widget.id, { show_hundredths: showHundredths })}
                />
              </div>
            </div>
          </>
        ) : (
          <SelectField
            label={t('widget-editor.format', 'Format')}
            value={widget.data.format}
            onValueChange={(value) => updateWidgetData(widget.id, { format: value })}
            options={translateOptions(TIME_FORMATS, t)}
          />
        )}
      </div>
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        showContentAlignment
      />
      <IconSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        setNumericField={setNumericField}
      />
    </>
  )
}

import { FolderOpen } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ButtonGroup } from '@/components/ui/button-group'
import { Input } from '@/components/ui/input'
import { useTranslation } from 'react-i18next'
import { useRasterSelection } from '../hooks/useRasterSelection'
import { DimensionsSection } from './widgetEditorSections'
import { SliderField } from './widgetFormControls'

/** @param {{widget: object, updateWidgetSize: Function, commitWidgetSize: Function, setNumericField: Function}} props */
export default function RasterWidgetEditor({ widget, updateWidgetSize, commitWidgetSize, setNumericField }) {
  const { t } = useTranslation()
  const { error, isLoading, selectImage } = useRasterSelection(widget.id)

  return (
    <div className="flex flex-col gap-2">
      <div className="mb-4">
        <DimensionsSection widget={widget} setNumericField={setNumericField} min={1} />
      </div>
      <SliderField
        label={t('widget-editor.rotation', 'Rotation')}
        value={widget.data.rotation}
        min={-180}
        max={180}
        step={1}
        valueDisplay={`${widget.data.rotation}°`}
        onSliderChange={(rotation) => updateWidgetSize(widget.id, { rotation })}
        onSliderCommit={() => commitWidgetSize(widget.id)}
      />
      <ButtonGroup className="w-full pt-4">
        <Input
          value={widget.data.path ?? ''}
          readOnly
          placeholder={t('raster.selectImage')}
          title={widget.data.path ?? undefined}
          className="h-9 min-w-0 flex-1 border-border/70 text-xs"
          aria-label={t('raster.imageFiles')}
        />
        <Button
          type="button"
          variant="outline"
          className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground "
          disabled={isLoading}
          onClick={selectImage}
          aria-label={t(widget.data.path ? 'raster.replaceImage' : 'raster.selectImage')}
        >
          <FolderOpen className="h-4 w-4" />
        </Button>
      </ButtonGroup>
      {error ? (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  )
}

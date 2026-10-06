import { FolderOpen, Play, Video } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ButtonGroup } from '@/components/ui/button-group'
import { DialogTitle } from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Slider } from '@/components/ui/slider'
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTranslation } from 'react-i18next'
import { BatchFolderPicker } from './BatchRenderQueue'
import ExportRangeSettings from './ExportRangeSettings'
import { QUALITY_SLIDER_RANGE } from '../data/qualityDefaults'

/**
 * Presents export settings and the output destination for either render target.
 * @param {object} props Render dialog state and handlers.
 * @returns {JSX.Element} Export settings panel.
 */
export default function RenderExportSettings(ctx) {
  const { t } = useTranslation()
  const isCompositeExport = ctx.exportMode === 'composite'

  return (
    <div className="min-w-0 space-y-8">
      <div className="space-y-2">
        <div className="flex items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <Video className="h-4 w-4 text-primary" />
            <DialogTitle className="text-sm font-semibold text-foreground">{t('render-video.exportSettings', 'Export Settings')}</DialogTitle>
          </div>

          {ctx.showExportModeOverride ? (
            <Tabs value={ctx.exportMode} onValueChange={ctx.handleExportModeChange}>
              <TabsList className="h-7 bg-surface p-0.5" variant="toolbar">
                <TabsTrigger value="transparent" className="px-2 text-[10px]" variant="toolbar" disabled={ctx.settingsLocked}>
                  {t('render-video.transparent', 'Transparent')}
                </TabsTrigger>
                <TabsTrigger value="composite" className="px-2 text-[10px]" variant="toolbar" disabled={ctx.settingsLocked}>
                  {t('render-video.fullVideo', 'Full Video')}
                </TabsTrigger>
              </TabsList>
            </Tabs>
          ) : null}
        </div>
      </div>

      <RenderTargetTabs renderTarget={ctx.settings.renderTarget} onRenderTargetChange={ctx.handleRenderTargetChange} disabled={ctx.settingsLocked} />

      <div className="grid gap-8 lg:grid-cols-1">
        <div className="space-y-2">
          <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">{t('render-video.framerate', 'Framerate')}</Label>
          {ctx.fpsLocked ? (
            <div className="flex h-9 items-center rounded-sm border border-border/70 bg-surface-elevated px-3 text-xs text-muted-foreground">
              {ctx.isBatchTarget
                ? t('render-video.lockedToEachVideoFps', "Locked to each video's FPS")
                : t('render-video.lockedToVideoFps', 'Locked to video FPS ({{fps}} fps)', { fps: Math.round(ctx.lockedVideoFps) })}
            </div>
          ) : (
            <Select value={ctx.fpsMode} onValueChange={ctx.handleFpsModeChange} disabled={ctx.settingsLocked}>
              <SelectTrigger className="h-9 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="24">24 fps</SelectItem>
                <SelectItem value="30">30 fps</SelectItem>
                <SelectItem value="60">60 fps</SelectItem>
                <SelectItem value="custom">{t('render-video.custom', 'Custom')}</SelectItem>
              </SelectContent>
            </Select>
          )}
        </div>

        {!ctx.fpsLocked && ctx.fpsMode === 'custom' && (
          <div className="space-y-2">
            <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
              {t('render-video.customFps', 'Custom FPS')}
            </Label>
            <BlurInput
              type="number"
              min={1}
              step={1}
              inputMode="numeric"
              value={ctx.settings.fps}
              onKeyDown={(event) => {
                if (['.', ',', 'e', 'E', '+', '-'].includes(event.key)) {
                  event.preventDefault()
                }
              }}
              onChange={(event) => ctx.handleCustomFpsChange(event.target.value)}
              className="h-9 text-xs"
              disabled={ctx.settingsLocked}
            />
          </div>
        )}

        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Label className="text-xs font-semibold">{t('render-video.widgetUpdateRate', 'Widget Update Rate')}</Label>
            </div>
          </div>
          <Tabs
            value={ctx.settings.widgetUpdateRate.toString()}
            onValueChange={(value) => ctx.onSettingsChange({ widgetUpdateRate: parseInt(value, 10) })}
          >
            <TabsList
              className="grid h-8 w-full bg-surface p-0.5"
              style={{
                gridTemplateColumns: `repeat(${ctx.updateRateOptions.length}, minmax(0, 1fr))`,
              }}
            >
              {ctx.updateRateOptions.map((rate) => (
                <TabsTrigger key={rate} value={rate.toString()} className="text-[10px]" disabled={ctx.settingsLocked}>
                  1/{rate}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          {ctx.showContainerFps ? (
            <p className="text-[10px] text-muted-foreground">
              {t('render-video.outputContainerFps', 'Output container: {{fps}} fps', {
                fps: ctx.containerFps.toFixed(2).replace(/\.00$/, ''),
              })}
            </p>
          ) : null}
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
              {t('render-video.codecOutputFormat', 'Codec / Output Format')}
            </Label>
            <Select value={ctx.selectedOutputFormatValue} onValueChange={ctx.handleOutputFormatChange} disabled={ctx.settingsLocked}>
              <SelectTrigger className="h-9 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectLabel className="flex items-center justify-between gap-3 text-[10px] font-bold uppercase tracking-widest">
                    <span>{t('render-video.transparentCodecs', 'Transparent Codecs')}</span>
                    {ctx.showVideoImportedBadge && (
                      <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[9px] normal-case tracking-normal text-primary">
                        {t('render-video.videoImported', 'Video imported')}
                      </span>
                    )}
                  </SelectLabel>
                  <SelectSeparator className="my-0" />
                  {ctx.OUTPUT_FORMATS.filter((option) => option.group === 'transparent').map((option) => (
                    <SelectItem key={option.value} value={option.value} disabled={isCompositeExport}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectGroup>

                <SelectGroup>
                  <SelectLabel className="mt-1 flex items-center justify-between gap-3 text-[10px] font-bold uppercase tracking-widest">
                    <span>{t('render-video.mp4Codecs', 'MP4 Codecs')}</span>
                    {ctx.showVideoRequiredBadge && (
                      <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[9px] normal-case tracking-normal text-primary">
                        {t('render-video.videoRequired', 'Video required')}
                      </span>
                    )}
                  </SelectLabel>
                  <SelectSeparator className="my-0" />
                  {ctx.OUTPUT_FORMATS.filter((option) => option.group === 'mp4').map((option) => {
                    const available = ctx.isOutputFormatAvailable(option, ctx.platformOs, ctx.availableCodecs)
                    const disabled = !isCompositeExport || !available
                    return (
                      <SelectItem key={option.value} value={option.value} disabled={disabled}>
                        <span className="flex w-full items-center justify-between gap-3">
                          <span className="min-w-0 truncate">{option.label}</span>
                          {!available && (
                            <span className="shrink-0 text-right text-[10px] text-muted-foreground">
                              {t('render-video.unavailable', 'Unavailable')}
                            </span>
                          )}
                        </span>
                      </SelectItem>
                    )
                  })}
                </SelectGroup>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
              {t('render-video.hardwareAcceleration', 'Hardware Acceleration')}
            </Label>
            <Select value={ctx.selectedAccelerationValue} onValueChange={ctx.handleAccelerationChange} disabled={ctx.settingsLocked}>
              <SelectTrigger className="h-9 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ctx.selectedAccelerationOptions.map((option) => (
                  <SelectItem key={option.value} value={option.value} disabled={!option.available}>
                    <span className="flex w-full items-center justify-between gap-3">
                      <span className="min-w-0 truncate">{option.label}</span>
                      {!option.available && (
                        <span className="shrink-0 text-right text-[10px] text-muted-foreground">{t('render-video.unavailable', 'Unavailable')}</span>
                      )}
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        {ctx.selectedCodecIsMp4 && (
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <Tabs value={ctx.settings.qualityType} onValueChange={ctx.handleQualityTypeChange}>
                <TabsList className="h-7 bg-surface p-0.5" variant="toolbar">
                  <TabsTrigger value="quality" className="px-2 text-[10px]" variant="toolbar">
                    {t('render-video.quality', 'Quality')}
                  </TabsTrigger>
                  <TabsTrigger value="bitrate" className="px-2 text-[10px]" variant="toolbar">
                    {t('render-video.bitrate', 'Bitrate')}
                  </TabsTrigger>
                </TabsList>
              </Tabs>
              <span className="rounded bg-surface-strong px-2 py-0.5 text-[10px] font-semibold text-muted-foreground tabular-nums">
                {ctx.settings.qualityType === 'quality' ? 'CRF ' : ''}
                {ctx.settings.qualityValue}
                {ctx.settings.qualityType === 'bitrate' ? ' Mbps' : ''}
              </span>
            </div>
            <div className="flex justify-between text-[10px] text-muted-foreground/70">
              <span>{ctx.settings.qualityType === 'quality' ? t('render-video.worse', 'Worse') : t('render-video.smallerFile', 'Smaller file')}</span>
              <span>{ctx.settings.qualityType === 'quality' ? t('render-video.better', 'Better') : t('render-video.largerFile', 'Larger file')}</span>
            </div>
            <Slider
              aria-label={ctx.settings.qualityType === 'quality' ? t('render-video.quality', 'Quality') : t('render-video.bitrate', 'Bitrate')}
              min={ctx.settings.qualityType === 'quality' ? QUALITY_SLIDER_RANGE.min : 5}
              max={ctx.settings.qualityType === 'quality' ? QUALITY_SLIDER_RANGE.max : 100}
              step={ctx.settings.qualityType === 'quality' ? 1 : 5}
              value={[ctx.qualitySliderValue]}
              onValueChange={ctx.handleQualityValueChange}
              disabled={ctx.settingsLocked}
            />
          </div>
        )}

        {ctx.showExportRangeSettings && (
          <ExportRangeSettings
            range={ctx.settings.range}
            onExportRangeChange={(range) => ctx.onSettingsChange({ range })}
            showUseVideoRangeAction={ctx.hasImportedVideo}
            onUseVideoRange={ctx.handleApplyImportedVideoRange}
          />
        )}

        {ctx.isBatchTarget ? (
          <div className="pt-4">
            <BatchFolderPicker
              label={t('render-video.outputFolder', 'Output folder')}
              folder={ctx.batchOutputFolder}
              onPick={ctx.pickOutputFolder}
              disabled={ctx.batchRunning}
            />
          </div>
        ) : (
          <div className="space-y-2 pt-4">
            <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
              {t('render-video.outputFile', 'Output file')}
            </Label>
            <ButtonGroup className="w-full">
              <BlurInput
                value={ctx.settings.outputPath}
                onBlur={(event) => ctx.handleOutputPathCommit(event.target.value)}
                className="h-9 min-w-0 flex-1 text-xs"
                aria-label={t('render-video.outputPath', 'Output path')}
              />
              <Button
                type="button"
                variant="outline"
                className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
                onClick={ctx.handleBrowse}
                disabled={ctx.submissionPending}
              >
                <FolderOpen className="h-4 w-4" />
              </Button>
            </ButtonGroup>
            {ctx.outputPathError ? <p className="text-xs text-red-500">{ctx.outputPathError}</p> : null}
          </div>
        )}
      </div>

      {!ctx.isBatchTarget && (
        <div className="flex items-center justify-end gap-3 pt-2">
          <Button
            type="button"
            variant="outline"
            className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
            onClick={ctx.onClose}
            disabled={ctx.renderingVideo || ctx.submissionPending}
          >
            {t('render-video.cancel', 'Cancel')}
          </Button>
          <Button
            type="button"
            className="bg-primary text-primary-foreground hover:bg-primary/90"
            onClick={ctx.onConfirm}
            disabled={ctx.renderStartDisabled}
          >
            <Play className="h-4 w-4" />
            {t('render-video.startRender', 'Start Render')}
          </Button>
        </div>
      )}
    </div>
  )
}

/**
 * Presents the current-video and batch target choices.
 * @param {object} props Target selection and change handler.
 * @returns {JSX.Element} Target tabs.
 */
export function RenderTargetTabs({ renderTarget, onRenderTargetChange, disabled = false }) {
  const { t } = useTranslation()
  return (
    <Tabs value={renderTarget} onValueChange={onRenderTargetChange}>
      <TabsList className="grid h-8 w-full grid-cols-2 bg-surface p-0.5">
        <TabsTrigger value="current" className="text-[10px]" disabled={disabled}>
          {t('render-video.currentVideo', 'Current video')}
        </TabsTrigger>
        <TabsTrigger value="batch" className="text-[10px]" disabled={disabled}>
          {t('render-video.batch', 'Batch')}
        </TabsTrigger>
      </TabsList>
    </Tabs>
  )
}

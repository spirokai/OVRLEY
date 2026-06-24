/**
 * Renders the render video dialog portion of the application interface.
 * Pure presentational - all logic is in useRenderVideoDialogState.
 */

import { AlertTriangle, Play, Video } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { timeToSeconds } from '@/features/overlay-editor/utils/exportRange'
import ExportRangeSettings from './ExportRangeSettings'
import RenderProgressPanel from './RenderProgressPanel'
import useRenderVideoDialogState from '../hooks/useRenderVideoDialogState'

/**
 * Renders the render video dialog component.
 *
 * @param {object} props - Component props.
 * @param {*} props.phase - Value for phase.
 * @param {*} props.settings - Value for settings.
 * @param {*} props.onSettingsChange - Callback invoked to settings change.
 * @param {*} props.onClose - Callback invoked to close.
 * @param {*} props.onConfirm - Callback invoked to confirm.
 * @returns {JSX.Element} Rendered component output.
 */
export default function RenderVideoDialog(props) {
  const ctx = useRenderVideoDialogState(props)

  if (ctx.phase === 'closed' || !ctx.settings) {
    return null
  }

  const isCompositeExport = ctx.exportMode === 'composite'
  const fps = isCompositeExport && ctx.importedVideoFps ? Math.round(ctx.importedVideoFps) : Number(ctx.settings.fps)
  const outputFormatLabel = ctx.OUTPUT_FORMATS.find((option) => option.value === ctx.selectedOutputFormatValue)?.label
  const accelerationLabel = ctx.selectedAccelerationOptions.find(
    (option) => option.value === ctx.selectedAccelerationValue && option.available && option.value !== 'cpu',
  )?.label
  const durationSeconds = isCompositeExport
    ? Number(ctx.importedVideoDuration)
    : ctx.settings.exportRange?.type === 'custom'
      ? timeToSeconds(ctx.settings.exportRange.toTime) - timeToSeconds(ctx.settings.exportRange.fromTime)
      : Number(ctx.config?.scene?.end) - Number(ctx.config?.scene?.start)
  const renderSummaryItems = [
    ctx.config?.scene?.width && ctx.config?.scene?.height ? `${ctx.config.scene.width}x${ctx.config.scene.height}` : null,
    Number.isFinite(fps) ? `${fps} fps` : null,
    Number.isFinite(Number(ctx.settings.updateRate)) ? `Update 1/${Number(ctx.settings.updateRate)}` : null,
    outputFormatLabel || ctx.settings.exportCodec || null,
    accelerationLabel || null,
    Number.isFinite(durationSeconds) && durationSeconds >= 0 ? formatDurationSummary(durationSeconds) : null,
  ].filter(Boolean)

  return (
    <div
      className="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/92 px-4 backdrop-blur-md"
      onMouseDown={ctx.handleBackdropPointerDown}
    >
      <div
        className="w-full max-w-md rounded-xl border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        onMouseDown={(event) => event.stopPropagation()}
      >
        {ctx.isProgress ? (
          <RenderProgressPanel renderProgress={ctx.renderProgress} renderSummaryItems={renderSummaryItems} onCancel={ctx.handleCancel} />
        ) : (
          <div className="space-y-6">
            <div className="space-y-2">
              <div className="flex items-start justify-between gap-4">
                <div className="flex items-center gap-3">
                  <Video className="h-4 w-4 text-primary" />
                  <h2 className="text-sm font-semibold text-foreground">{ctx.dialogTitle}</h2>
                </div>

                {ctx.showExportModeOverride ? (
                  <div className="flex items-center gap-2 pt-0.5">
                    <Switch
                      id="transparent-export-switch"
                      aria-label="Transparent Export"
                      checked={ctx.exportMode === 'transparent'}
                      onCheckedChange={ctx.handleExportModeChange}
                    />
                  </div>
                ) : null}
              </div>
            </div>

            <div className="grid gap-6 lg:grid-cols-1">
              <div className="space-y-2">
                <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">Framerate</Label>
                {isCompositeExport && ctx.importedVideoFps ? (
                  <div className="flex h-9 items-center rounded-md border border-border/70 bg-surface-elevated px-3 text-xs text-muted-foreground">
                    Locked to video FPS ({Math.round(ctx.importedVideoFps)} fps)
                  </div>
                ) : (
                  <Select value={ctx.fpsMode} onValueChange={ctx.handleFpsModeChange}>
                    <SelectTrigger className="h-9 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="24">24 fps</SelectItem>
                      <SelectItem value="30">30 fps</SelectItem>
                      <SelectItem value="60">60 fps</SelectItem>
                      <SelectItem value="custom">Custom</SelectItem>
                    </SelectContent>
                  </Select>
                )}
              </div>

              {(!isCompositeExport || !ctx.importedVideoFps) && ctx.fpsMode === 'custom' && (
                <div className="space-y-2">
                  <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">Custom FPS</Label>
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
                  />
                </div>
              )}

              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Label className="text-xs font-semibold">Widget Update Rate</Label>
                  </div>
                </div>
                <Tabs value={ctx.settings.updateRate.toString()} onValueChange={(value) => ctx.onSettingsChange({ updateRate: parseInt(value, 10) })}>
                  <TabsList
                    className="grid h-8 w-full bg-surface p-0.5"
                    style={{
                      gridTemplateColumns: `repeat(${ctx.updateRateOptions.length}, minmax(0, 1fr))`,
                    }}
                  >
                    {ctx.updateRateOptions.map((rate) => (
                      <TabsTrigger key={rate} value={rate.toString()} className="text-[10px]">
                        1/{rate}
                      </TabsTrigger>
                    ))}
                  </TabsList>
                </Tabs>
                <p className="text-[10px] text-muted-foreground">Output container: {ctx.containerFps.toFixed(2).replace(/\.00$/, '')} fps</p>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">Codec / Output Format</Label>
                  <Select value={ctx.selectedOutputFormatValue} onValueChange={ctx.handleOutputFormatChange}>
                    <SelectTrigger className="h-9 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        <SelectLabel className="flex items-center justify-between gap-3 text-[10px] font-bold uppercase tracking-widest">
                          <span>Transparent Codecs</span>
                          {ctx.hasImportedVideo && (
                            <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[9px] normal-case tracking-normal text-primary">
                              Video imported
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
                          <span>MP4 Codecs</span>
                          {!ctx.hasImportedVideo && (
                            <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[9px] normal-case tracking-normal text-primary">
                              Video required
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
                                {!available && <span className="shrink-0 text-right text-[10px] text-muted-foreground">Unavailable</span>}
                              </span>
                            </SelectItem>
                          )
                        })}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-2">
                  <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">Hardware Acceleration</Label>
                  <Select value={ctx.selectedAccelerationValue} onValueChange={ctx.handleAccelerationChange}>
                    <SelectTrigger className="h-9 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {ctx.selectedAccelerationOptions.map((option) => (
                        <SelectItem key={option.value} value={option.value} disabled={!option.available}>
                          <span className="flex w-full items-center justify-between gap-3">
                            <span className="min-w-0 truncate">{option.label}</span>
                            {!option.available && <span className="shrink-0 text-right text-[10px] text-muted-foreground">Unavailable</span>}
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
                    <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">Bitrate</Label>
                    <span className="rounded bg-surface-strong px-2 py-0.5 text-[10px] font-semibold text-muted-foreground">
                      {ctx.settings.exportBitrate ?? 20} Mbps
                    </span>
                  </div>
                  <Slider
                    min={5}
                    max={100}
                    step={5}
                    value={[ctx.settings.exportBitrate ?? 20]}
                    onValueChange={([value]) => ctx.onSettingsChange({ exportBitrate: value })}
                  />
                </div>
              )}

              {ctx.hasImportedVideo && ctx.resolutionMismatch && (
                <div className="flex items-start gap-2 rounded-md bg-destructive/10 p-2 text-destructive">
                  <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
                  <p className="text-[10px] leading-tight">
                    Overlay resolution {ctx.config?.scene?.width}x{ctx.config?.scene?.height} must match imported video{' '}
                    {ctx.importedVideoResolution?.width}x{ctx.importedVideoResolution?.height}.
                  </p>
                </div>
              )}

              {ctx.showExportRangeSettings && (
                <ExportRangeSettings
                  exportRange={ctx.settings.exportRange}
                  onExportRangeChange={(exportRange) => ctx.onSettingsChange({ exportRange })}
                  showUseVideoRangeAction={ctx.hasImportedVideo}
                  onUseVideoRange={ctx.handleApplyImportedVideoRange}
                />
              )}
            </div>

            <div className="flex items-center justify-end gap-3 pt-6">
              <Button
                type="button"
                variant="outline"
                className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
                onClick={ctx.onClose}
                disabled={ctx.renderingVideo}
              >
                Cancel
              </Button>
              <Button
                type="button"
                className="bg-primary text-primary-foreground hover:bg-primary/90"
                onClick={ctx.onConfirm}
                disabled={ctx.renderStartDisabled}
              >
                <Play className="h-4 w-4" />
                Start Render
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

function formatDurationSummary(durationSeconds) {
  const roundedSeconds = Math.round(durationSeconds)
  const minutes = Math.floor(roundedSeconds / 60)
  const seconds = roundedSeconds % 60

  if (minutes > 0) {
    return `${minutes} min ${seconds} sec`
  }

  return `${seconds} sec`
}

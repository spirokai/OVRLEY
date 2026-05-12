/**
 * Renders the render video dialog portion of the application interface.
 */

import { useCallback, useEffect, useMemo, useState } from 'react'
import { AlertTriangle, Film, Loader2, Play, Timer, Video } from 'lucide-react'
import { cancelRender } from '@/api/backend'
import useStore from '@/store/useStore'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Slider } from '@/components/ui/slider'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Progress } from '@/components/ui/progress'
import ExportRangeSettings from '@/components/ExportRangeSettings'
import {
  getContainerFps,
  getUpdateRateOptions,
  normalizeUpdateRateForFps,
  sanitizeIntegerFps,
} from '@/lib/update-rate'
import { getDefaultBitrate } from '@/lib/bitrateDefaults'

const OUTPUT_FORMATS = [
  {
    value: 'prores',
    label: 'ProRes',
    group: 'transparent',
    codecs: {
      cpu: 'prores_ks',
      videotoolbox: 'prores_videotoolbox',
      vulkan_prores: 'prores_ks_vulkan',
    },
  },
  {
    value: 'qtrle',
    label: 'QT RLE',
    group: 'transparent',
    codecs: {
      cpu: 'qtrle',
    },
  },
  {
    value: 'h264',
    label: 'H.264',
    group: 'mp4',
    codecs: {
      cpu: 'libx264',
      nvidia: 'h264_nvenc',
      nvidia_cuda: 'h264_nvenc',
      qsv: 'h264_qsv',
      amd: 'h264_amf',
      videotoolbox: 'h264_videotoolbox',
      vaapi: 'h264_vaapi',
    },
  },
  {
    value: 'hevc',
    label: 'H.265 / HEVC',
    group: 'mp4',
    codecs: {
      cpu: 'libx265',
      nvidia: 'hevc_nvenc',
      nvidia_cuda: 'hevc_nvenc',
      qsv: 'hevc_qsv',
      amd: 'hevc_amf',
      videotoolbox: 'hevc_videotoolbox',
      vaapi: 'hevc_vaapi',
    },
  },
]

const ACCELERATION_OPTIONS = [
  { value: 'cpu', label: 'CPU' },
  { value: 'nvidia', label: 'NVIDIA GPU', platform: ['windows', 'linux'] },
  {
    value: 'nvidia_cuda',
    label: 'NVIDIA GPU | CUDA',
    platform: ['windows', 'linux'],
  },
  { value: 'qsv', label: 'Intel Quick Sync', platform: ['windows', 'linux'] },
  { value: 'amd', label: 'AMD GPU', platform: ['windows', 'linux'] },
  {
    value: 'videotoolbox',
    label: 'Apple VideoToolbox',
    platform: ['macos'],
  },
  { value: 'vaapi', label: 'VAAPI', platform: ['linux'] },
  { value: 'vulkan_prores', label: 'Vulkan' },
]

const OUTPUT_FORMATS_BY_VALUE = Object.fromEntries(
  OUTPUT_FORMATS.map((option) => [option.value, option]),
)

const EXPORT_CODEC_LOOKUP = OUTPUT_FORMATS.flatMap((format) =>
  Object.entries(format.codecs).map(([acceleration, codec]) => ({
    codec,
    format: format.value,
    acceleration,
  })),
).reduce((lookup, item) => {
  if (!lookup[item.codec]) {
    lookup[item.codec] = item
  }
  return lookup
}, {})

const LEGACY_MP4_CODECS = ['h264_vaapi', 'hevc_vaapi']

function getOutputFormatForExportCodec(codec) {
  return OUTPUT_FORMATS_BY_VALUE[EXPORT_CODEC_LOOKUP[codec]?.format] || null
}

function getExportCodecForSelection(formatValue, accelerationValue) {
  return (
    OUTPUT_FORMATS_BY_VALUE[formatValue]?.codecs?.[accelerationValue] || null
  )
}

function isMp4Codec(codec) {
  return (
    getOutputFormatForExportCodec(codec)?.group === 'mp4' ||
    LEGACY_MP4_CODECS.includes(codec)
  )
}

function codecFlag(availableCodecs, codec) {
  const flagByCodec = {
    libx264: 'libx264',
    libx265: 'libx265',
    h264_nvenc: 'h264Nvenc',
    hevc_nvenc: 'hevcNvenc',
    h264_qsv: 'h264Qsv',
    hevc_qsv: 'hevcQsv',
    h264_amf: 'h264Amf',
    hevc_amf: 'hevcAmf',
    h264_videotoolbox: 'h264Videotoolbox',
    hevc_videotoolbox: 'hevcVideotoolbox',
  }
  const key = flagByCodec[codec]
  return Boolean(availableCodecs?.[key])
}

function isAccelerationPotentiallyVisible(option, platformOs) {
  if (!option?.platform || platformOs === 'unknown') return true
  return option.platform.includes(platformOs)
}

function isAccelerationAvailable(format, accelerationValue, availableCodecs) {
  const codec = getExportCodecForSelection(format.value, accelerationValue)
  if (!codec) return false

  if (format.group === 'transparent') {
    return true
  }

  if (!availableCodecs) return false

  if (accelerationValue === 'nvidia') {
    return (
      codecFlag(availableCodecs, codec) &&
      Boolean(availableCodecs.nvgpu || availableCodecs.nnvgpu)
    )
  }

  if (accelerationValue === 'nvidia_cuda') {
    return (
      codecFlag(availableCodecs, codec) &&
      Boolean(availableCodecs.nnvgpu || availableCodecs.cuda)
    )
  }

  if (accelerationValue === 'qsv') {
    return codecFlag(availableCodecs, codec) && Boolean(availableCodecs.qsv)
  }

  if (accelerationValue === 'amd') {
    return codecFlag(availableCodecs, codec)
  }

  if (accelerationValue === 'videotoolbox') {
    return (
      codecFlag(availableCodecs, codec) && Boolean(availableCodecs.videotoolbox)
    )
  }

  return codecFlag(availableCodecs, codec)
}

function getVisibleAccelerationOptions(format, platformOs, availableCodecs) {
  return ACCELERATION_OPTIONS.map((option) => {
    const codecSupported = Object.hasOwn(format.codecs, option.value)
    const platformVisible = isAccelerationPotentiallyVisible(option, platformOs)
    return {
      ...option,
      codecSupported,
      available:
        codecSupported &&
        platformVisible &&
        isAccelerationAvailable(format, option.value, availableCodecs),
      platformVisible,
    }
  })
}

function getFirstAvailableAcceleration(format, platformOs, availableCodecs) {
  return getVisibleAccelerationOptions(
    format,
    platformOs,
    availableCodecs,
  ).find((option) => option.available)
}

function isOutputFormatAvailable(format, platformOs, availableCodecs) {
  if (format.group === 'transparent') {
    return true
  }

  return Boolean(
    getFirstAvailableAcceleration(format, platformOs, availableCodecs),
  )
}

function getFirstAvailableMp4ExportCodec(platformOs, availableCodecs) {
  for (const format of OUTPUT_FORMATS.filter(
    (option) => option.group === 'mp4',
  )) {
    const acceleration = getFirstAvailableAcceleration(
      format,
      platformOs,
      availableCodecs,
    )
    if (acceleration) {
      return getExportCodecForSelection(format.value, acceleration.value)
    }
  }

  return null
}

function getAccelerationValueForSettings(settings) {
  const format = getOutputFormatForExportCodec(settings?.exportCodec)
  if (!format) return 'cpu'

  if (
    settings?.exportAcceleration &&
    getExportCodecForSelection(format.value, settings.exportAcceleration) ===
      settings.exportCodec
  ) {
    return settings.exportAcceleration
  }

  return EXPORT_CODEC_LOOKUP[settings.exportCodec]?.acceleration || 'cpu'
}

function resolutionsMismatch(scene, videoResolution) {
  if (!scene?.width || !scene?.height || !videoResolution) {
    return false
  }

  return (
    Number(scene.width) !== Number(videoResolution.width) ||
    Number(scene.height) !== Number(videoResolution.height)
  )
}

/**
 * Formats time.
 *
 * @param {*} seconds - Numeric seconds value.
 * @returns {string} Formatted representation of the input.
 */
function formatTime(seconds) {
  if (seconds === null || seconds === undefined) {
    return '--:--'
  }

  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

/**
 * Renders the render progress panel component.
 * @returns {JSX.Element} Rendered component output.
 */
function RenderProgressPanel() {
  const renderProgress = useStore((state) => state.renderProgress)
  const [isCancelling, setIsCancelling] = useState(false)

  const {
    percent,
    current,
    total,
    message,
    estimatedSecondsRemaining,
    encoded,
  } = renderProgress

  useEffect(() => {
    if (renderProgress.status !== 'rendering') {
      setIsCancelling(false)
    }
  }, [renderProgress.status])

  const handleCancel = async () => {
    try {
      setIsCancelling(true)
      await cancelRender()
    } catch (error) {
      console.error('Failed to cancel render:', error)
      setIsCancelling(false)
    }
  }

  const isFinalizing = percent >= 100

  let subMessage = message || 'Processing frames...'
  if (isFinalizing) {
    subMessage =
      encoded && total > 0
        ? `Encoding: ${encoded.toLocaleString()} / ${total.toLocaleString()} frames`
        : 'Encoding output file...'
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col items-center gap-4 text-center">
        <div className="relative flex h-16 w-16 items-center justify-center rounded-full bg-surface-accent-soft">
          <Loader2 className="absolute h-10 w-10 animate-spin text-primary" />
          <Film className="h-5 w-5 text-primary/60" />
        </div>
        <div>
          <h2 className="text-xl font-bold text-foreground">
            {isFinalizing ? 'Finalizing Video' : 'Exporting Overlay'}
          </h2>
          <p className="mt-1 text-sm text-muted-foreground">{subMessage}</p>
        </div>
      </div>

      <div className="space-y-3">
        <div className="flex justify-between text-xs font-medium">
          <span className="text-primary">{percent}% Complete</span>
          <span className="text-muted-foreground">
            {current.toLocaleString()} / {total.toLocaleString()} frames
          </span>
        </div>
        <Progress value={percent} className="h-2 bg-surface-strong" />
      </div>

      {!isFinalizing && (
        <div className="flex items-center justify-center gap-6 pt-2">
          <div className="flex flex-col items-center">
            <div className="mb-1 flex items-center gap-1.5 text-muted-foreground">
              <Timer className="h-3.5 w-3.5" />
              <span className="text-[10px] font-bold uppercase tracking-wider">
                Est. Remaining
              </span>
            </div>
            <span className="text-lg font-mono font-bold text-foreground">
              {formatTime(estimatedSecondsRemaining)}
            </span>
          </div>
        </div>
      )}

      <div className="flex justify-center pt-2">
        <Button
          type="button"
          variant="ghost"
          className="text-muted-foreground hover:bg-surface-accent-soft hover:text-highlight"
          onClick={handleCancel}
          disabled={isCancelling}
        >
          {isCancelling ? (
            <>
              <Loader2 className="h-3 w-3 animate-spin" />
              Cancelling...
            </>
          ) : (
            'Cancel'
          )}
        </Button>
      </div>

      <p className="text-center text-[10px] italic text-muted-foreground/50">
        Please keep the application open during rendering
      </p>
    </div>
  )
}

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
export default function RenderVideoDialog({
  phase,
  settings,
  onSettingsChange,
  onClose,
  onConfirm,
}) {
  const renderingVideo = useStore((state) => state.renderingVideo)
  const platformOs = useStore((state) => state.platformOs)
  const availableCodecs = useStore((state) => state.availableCodecs)
  const config = useStore((state) => state.config)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoFps = useStore((state) => state.importedVideoFps)
  const importedVideoResolution = useStore(
    (state) => state.importedVideoResolution,
  )
  const [fpsMode, setFpsMode] = useState(
    [24, 30, 60].includes(settings?.fps) ? settings.fps.toString() : 'custom',
  )
  const updateRateOptions = useMemo(
    () => getUpdateRateOptions(settings?.fps),
    [settings?.fps],
  )
  const containerFps = useMemo(
    () => getContainerFps(settings?.fps, settings?.updateRate),
    [settings?.fps, settings?.updateRate],
  )
  const hasImportedVideo = Boolean(importedVideoPath)
  const selectedOutputFormat = getOutputFormatForExportCodec(
    settings?.exportCodec,
  )
  const selectedOutputFormatValue = selectedOutputFormat?.value || 'prores'
  const selectedAccelerationValue = getAccelerationValueForSettings(settings)
  const selectedAccelerationOptions = useMemo(
    () =>
      getVisibleAccelerationOptions(
        OUTPUT_FORMATS_BY_VALUE[selectedOutputFormatValue],
        platformOs,
        availableCodecs,
      ),
    [availableCodecs, platformOs, selectedOutputFormatValue],
  )
  const selectedCodecIsMp4 = isMp4Codec(settings?.exportCodec)
  const selectedAccelerationAvailable = Boolean(
    selectedAccelerationOptions.find(
      (option) => option.value === selectedAccelerationValue,
    )?.available,
  )
  const selectedExportCodecAvailable =
    Boolean(EXPORT_CODEC_LOOKUP[settings?.exportCodec]) &&
    selectedAccelerationAvailable
  const resolutionMismatch = resolutionsMismatch(
    config?.scene,
    importedVideoResolution,
  )
  const renderStartDisabled =
    renderingVideo ||
    resolutionMismatch ||
    (hasImportedVideo &&
      (!selectedCodecIsMp4 || !selectedExportCodecAvailable)) ||
    (!hasImportedVideo && selectedCodecIsMp4)

  const defaultBitrateForCodec = useCallback(
    (codec) =>
      getDefaultBitrate(
        importedVideoResolution?.width || config?.scene?.width,
        importedVideoResolution?.height || config?.scene?.height,
        importedVideoFps || settings?.fps,
        codec,
      ),
    [
      config?.scene?.height,
      config?.scene?.width,
      importedVideoFps,
      importedVideoResolution?.height,
      importedVideoResolution?.width,
      settings?.fps,
    ],
  )

  useEffect(() => {
    if (!settings) {
      return
    }

    if ([24, 30, 60].includes(settings.fps)) {
      setFpsMode(settings.fps.toString())
      return
    }

    setFpsMode('custom')
  }, [settings])

  useEffect(() => {
    if (!settings) {
      return
    }

    if (!hasImportedVideo && selectedCodecIsMp4) {
      onSettingsChange({
        exportCodec: 'prores_ks',
        exportAcceleration: 'cpu',
        exportBitrate: undefined,
      })
      return
    }

    if (!hasImportedVideo) {
      return
    }

    const firstAvailableMp4Codec = getFirstAvailableMp4ExportCodec(
      platformOs,
      availableCodecs,
    )

    if (!selectedCodecIsMp4 || !selectedExportCodecAvailable) {
      if (firstAvailableMp4Codec) {
        onSettingsChange({
          exportCodec: firstAvailableMp4Codec,
          exportAcceleration:
            EXPORT_CODEC_LOOKUP[firstAvailableMp4Codec]?.acceleration || 'cpu',
          exportBitrate: defaultBitrateForCodec(firstAvailableMp4Codec),
        })
      }
      return
    }

    if (!Number.isFinite(settings.exportBitrate)) {
      onSettingsChange({
        exportBitrate: defaultBitrateForCodec(settings.exportCodec),
      })
    }
  }, [
    hasImportedVideo,
    defaultBitrateForCodec,
    onSettingsChange,
    platformOs,
    selectedCodecIsMp4,
    selectedExportCodecAvailable,
    settings,
    availableCodecs,
  ])

  useEffect(() => {
    if (!settings) {
      return
    }

    const normalizedUpdateRate = normalizeUpdateRateForFps(
      settings.fps,
      settings.updateRate,
    )
    if (normalizedUpdateRate !== settings.updateRate) {
      onSettingsChange({ updateRate: normalizedUpdateRate })
    }
  }, [settings, onSettingsChange])

  if (phase === 'closed' || !settings) {
    return null
  }

  const isProgress = phase === 'progress'
  const handleBackdropPointerDown = (event) => {
    if (isProgress || event.target !== event.currentTarget) {
      return
    }

    onClose()
  }
  const handleOutputFormatChange = (value) => {
    const format = OUTPUT_FORMATS_BY_VALUE[value]
    if (!format) return

    const acceleration =
      getVisibleAccelerationOptions(format, platformOs, availableCodecs).find(
        (option) =>
          option.value === selectedAccelerationValue && option.available,
      ) || getFirstAvailableAcceleration(format, platformOs, availableCodecs)

    if (!acceleration) return

    const nextExportCodec = getExportCodecForSelection(
      format.value,
      acceleration.value,
    )
    const nextIsMp4Codec = format.group === 'mp4'

    onSettingsChange({
      exportCodec: nextExportCodec,
      exportAcceleration: acceleration.value,
      exportBitrate: nextIsMp4Codec
        ? defaultBitrateForCodec(nextExportCodec)
        : undefined,
    })
  }
  const handleAccelerationChange = (value) => {
    const nextExportCodec = getExportCodecForSelection(
      selectedOutputFormatValue,
      value,
    )
    if (!nextExportCodec) return

    onSettingsChange({
      exportCodec: nextExportCodec,
      exportAcceleration: value,
      exportBitrate: selectedCodecIsMp4
        ? defaultBitrateForCodec(nextExportCodec)
        : undefined,
    })
  }

  return (
    <div
      className="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/92 px-4 backdrop-blur-md"
      onMouseDown={handleBackdropPointerDown}
    >
      <div
        className="w-full max-w-md rounded-xl border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        onMouseDown={(event) => event.stopPropagation()}
      >
        {isProgress ? (
          <RenderProgressPanel />
        ) : (
          <div className="space-y-6">
            <div className="space-y-2">
              <div className="flex items-center gap-3">
                <Video className="h-4 w-4 text-primary" />
                <h2 className="text-sm font-semibold text-foreground">
                  Select Render Settings
                </h2>
              </div>
            </div>

            <div className="grid gap-6 lg:grid-cols-1">
              <div className="space-y-2">
                <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                  Framerate
                </Label>
                {importedVideoFps ? (
                  <div className="flex h-9 items-center rounded-md border border-border/70 bg-surface-elevated px-3 text-xs text-muted-foreground">
                    Locked to video FPS ({Math.round(importedVideoFps)} fps)
                  </div>
                ) : (
                  <Select
                    value={fpsMode}
                    onValueChange={(value) => {
                      setFpsMode(value)
                      if (value !== 'custom') {
                        const fps = sanitizeIntegerFps(value)
                        onSettingsChange({
                          fps,
                          updateRate: normalizeUpdateRateForFps(
                            fps,
                            settings.updateRate,
                          ),
                        })
                      }
                    }}
                  >
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

              {!importedVideoFps && fpsMode === 'custom' && (
                <div className="space-y-2">
                  <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                    Custom FPS
                  </Label>
                  <BlurInput
                    type="number"
                    min={1}
                    step={1}
                    inputMode="numeric"
                    value={settings.fps}
                    onKeyDown={(event) => {
                      if (['.', ',', 'e', 'E', '+', '-'].includes(event.key)) {
                        event.preventDefault()
                      }
                    }}
                    onChange={(event) => {
                      const fps = sanitizeIntegerFps(event.target.value)
                      onSettingsChange({
                        fps,
                        updateRate: normalizeUpdateRateForFps(
                          fps,
                          settings.updateRate,
                        ),
                      })
                    }}
                    className="h-9 text-xs"
                  />
                </div>
              )}

              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Label className="text-xs font-semibold">
                      Widget Update Rate
                    </Label>
                  </div>
                </div>
                <Tabs
                  value={settings.updateRate.toString()}
                  onValueChange={(value) =>
                    onSettingsChange({ updateRate: parseInt(value, 10) })
                  }
                >
                  <TabsList
                    className="grid h-8 w-full bg-surface p-0.5"
                    style={{
                      gridTemplateColumns: `repeat(${updateRateOptions.length}, minmax(0, 1fr))`,
                    }}
                  >
                    {updateRateOptions.map((rate) => (
                      <TabsTrigger
                        key={rate}
                        value={rate.toString()}
                        className="text-[10px]"
                      >
                        1/{rate}
                      </TabsTrigger>
                    ))}
                  </TabsList>
                </Tabs>
                <p className="text-[10px] text-muted-foreground">
                  Output container:{' '}
                  {containerFps.toFixed(2).replace(/\.00$/, '')} fps
                </p>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                    Codec / Output Format
                  </Label>
                  <Select
                    value={selectedOutputFormatValue}
                    onValueChange={handleOutputFormatChange}
                  >
                    <SelectTrigger className="h-9 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        <SelectLabel className="flex items-center justify-between gap-3 text-[10px] font-bold uppercase tracking-widest">
                          <span>Transparent Codecs</span>
                          {hasImportedVideo && (
                            <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[9px] normal-case tracking-normal text-primary">
                              Video imported
                            </span>
                          )}
                        </SelectLabel>
                        <SelectSeparator className="my-0" />
                        {OUTPUT_FORMATS.filter(
                          (option) => option.group === 'transparent',
                        ).map((option) => (
                          <SelectItem
                            key={option.value}
                            value={option.value}
                            disabled={hasImportedVideo}
                          >
                            {option.label}
                          </SelectItem>
                        ))}
                      </SelectGroup>

                      <SelectGroup>
                        <SelectLabel className="mt-1 flex items-center justify-between gap-3 text-[10px] font-bold uppercase tracking-widest">
                          <span>MP4 Codecs</span>
                          {!hasImportedVideo && (
                            <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[9px] normal-case tracking-normal text-primary">
                              Video required
                            </span>
                          )}
                        </SelectLabel>
                        <SelectSeparator className="my-0" />
                        {OUTPUT_FORMATS.filter(
                          (option) => option.group === 'mp4',
                        ).map((option) => {
                          const available = isOutputFormatAvailable(
                            option,
                            platformOs,
                            availableCodecs,
                          )
                          const disabled = !hasImportedVideo || !available
                          return (
                            <SelectItem
                              key={option.value}
                              value={option.value}
                              disabled={disabled}
                            >
                              <span className="flex w-full items-center justify-between gap-3">
                                <span className="min-w-0 truncate">
                                  {option.label}
                                </span>
                                {!available && (
                                  <span className="shrink-0 text-right text-[10px] text-muted-foreground">
                                    Unavailable
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
                    Hardware Acceleration
                  </Label>
                  <Select
                    value={selectedAccelerationValue}
                    onValueChange={handleAccelerationChange}
                  >
                    <SelectTrigger className="h-9 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {selectedAccelerationOptions.map(
                        (option) =>
                          (option.available || option.platformVisible) && (
                            <SelectItem
                              key={option.value}
                              value={option.value}
                              disabled={!option.available}
                            >
                              <span className="flex w-full items-center justify-between gap-3">
                                <span className="min-w-0 truncate">
                                  {option.label}
                                </span>
                                {!option.available && (
                                  <span className="shrink-0 text-right text-[10px] text-muted-foreground">
                                    {!option.codecSupported
                                      ? 'Unsupported'
                                      : option.platformVisible
                                        ? 'Unavailable'
                                        : 'Unsupported'}
                                  </span>
                                )}
                              </span>
                            </SelectItem>
                          ),
                      )}
                    </SelectContent>
                  </Select>
                </div>
              </div>

              {selectedCodecIsMp4 && (
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                      Bitrate
                    </Label>
                    <span className="rounded bg-surface-strong px-2 py-0.5 text-[10px] font-semibold text-muted-foreground">
                      {settings.exportBitrate ?? 20} Mbps
                    </span>
                  </div>
                  <Slider
                    min={5}
                    max={100}
                    step={5}
                    value={[settings.exportBitrate ?? 20]}
                    onValueChange={([value]) =>
                      onSettingsChange({ exportBitrate: value })
                    }
                  />
                </div>
              )}

              {hasImportedVideo && resolutionMismatch && (
                <div className="flex items-start gap-2 rounded-md bg-destructive/10 p-2 text-destructive">
                  <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
                  <p className="text-[10px] leading-tight">
                    Overlay resolution {config?.scene?.width}x
                    {config?.scene?.height} must match imported video{' '}
                    {importedVideoResolution?.width}x
                    {importedVideoResolution?.height}.
                  </p>
                </div>
              )}

              {!hasImportedVideo && (
                <ExportRangeSettings
                  exportRange={settings.exportRange}
                  onExportRangeChange={(exportRange) =>
                    onSettingsChange({ exportRange })
                  }
                />
              )}
            </div>

            <div className="flex items-center justify-end gap-3 pt-6">
              <Button
                type="button"
                variant="outline"
                className="border-border/70 bg-surface text-foreground hover:bg-surface-elevated"
                onClick={onClose}
                disabled={renderingVideo}
              >
                Cancel
              </Button>
              <Button
                type="button"
                className="bg-primary text-primary-foreground hover:bg-primary/90"
                onClick={onConfirm}
                disabled={renderStartDisabled}
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

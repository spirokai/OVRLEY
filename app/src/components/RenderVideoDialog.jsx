import { useEffect, useState } from 'react'
import { Film, Loader2, Play, Timer, Video } from 'lucide-react'
import { cancelRender } from '@/api/backend'
import useStore from '@/store/useStore'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Switch } from '@/components/ui/switch'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Progress } from '@/components/ui/progress'

function formatTime(seconds) {
  if (seconds === null || seconds === undefined) {
    return '--:--'
  }

  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

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
            {isFinalizing ? 'Finalizing Video' : 'Generating Overlay'}
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

export default function RenderVideoDialog({
  phase,
  settings,
  onSettingsChange,
  onClose,
  onConfirm,
}) {
  const renderingVideo = useStore((state) => state.renderingVideo)
  const platformOs = useStore((state) => state.platformOs)
  const [fpsMode, setFpsMode] = useState(
    [24, 30, 60].includes(settings?.fps) ? settings.fps.toString() : 'custom',
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

  if (phase === 'closed' || !settings) {
    return null
  }

  const isProgress = phase === 'progress'
  const isVideoToolboxAvailable = platformOs === 'macos'

  return (
    <div
      className="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/92 px-4 backdrop-blur-md"
      onClick={isProgress ? undefined : onClose}
    >
      <div
        className="w-full max-w-md rounded-xl border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        onClick={(event) => event.stopPropagation()}
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
                <Select
                  value={fpsMode}
                  onValueChange={(value) => {
                    setFpsMode(value)
                    if (value !== 'custom') {
                      onSettingsChange({ fps: parseInt(value, 10) })
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
              </div>

              {fpsMode === 'custom' && (
                <div className="space-y-2">
                  <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                    Custom FPS
                  </Label>
                  <BlurInput
                    type="number"
                    min={1}
                    value={settings.fps}
                    onChange={(event) =>
                      onSettingsChange({
                        fps: Math.max(parseInt(event.target.value, 10) || 1, 1),
                      })
                    }
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
                  <TabsList className="grid h-8 w-full grid-cols-4 bg-surface p-0.5">
                    <TabsTrigger value="1" className="text-[10px]">
                      1/1
                    </TabsTrigger>
                    <TabsTrigger value="2" className="text-[10px]">
                      1/2
                    </TabsTrigger>
                    <TabsTrigger value="4" className="text-[10px]">
                      1/4
                    </TabsTrigger>
                    <TabsTrigger value="8" className="text-[10px]">
                      1/8
                    </TabsTrigger>
                  </TabsList>
                </Tabs>
              </div>

              <div className="space-y-2">
                <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                  Export Codec
                </Label>
                <Select
                  value={settings.exportCodec}
                  onValueChange={(value) =>
                    onSettingsChange({ exportCodec: value })
                  }
                >
                  <SelectTrigger className="h-9 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="prores_ks">ProRes (CPU)</SelectItem>
                    <SelectItem value="prores_ks_vulkan">
                      ProRes Vulkan (GPU)
                    </SelectItem>
                    <SelectItem
                      value="prores_videotoolbox"
                      disabled={!isVideoToolboxAvailable}
                    >
                      ProRes (macOS)
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <Label className="text-xs font-medium">
                      Custom Export Range
                    </Label>
                  </div>
                  <Switch
                    checked={settings.exportRange.type === 'custom'}
                    onCheckedChange={(checked) =>
                      onSettingsChange({
                        exportRange: {
                          ...settings.exportRange,
                          type: checked ? 'custom' : 'all',
                        },
                      })
                    }
                  />
                </div>

                {settings.exportRange.type === 'custom' && (
                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-1.5">
                      <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                        From
                      </Label>
                      <BlurInput
                        value={settings.exportRange.fromTime}
                        onChange={(event) =>
                          onSettingsChange({
                            exportRange: {
                              ...settings.exportRange,
                              fromTime: event.target.value,
                            },
                          })
                        }
                        className="h-9 text-xs font-mono"
                        placeholder="00:00:00 or 800"
                      />
                    </div>

                    <div className="space-y-1.5">
                      <Label className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                        To
                      </Label>
                      <BlurInput
                        value={settings.exportRange.toTime}
                        onChange={(event) =>
                          onSettingsChange({
                            exportRange: {
                              ...settings.exportRange,
                              toTime: event.target.value,
                            },
                          })
                        }
                        className="h-9 text-xs font-mono"
                        placeholder="00:00:00 or 900"
                      />
                    </div>
                  </div>
                )}
              </div>
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
                disabled={renderingVideo}
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

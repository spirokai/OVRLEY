import { Bell, ChevronDown, ChevronUp, Clock3, RotateCcw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { BlurInput } from '@/components/ui/blur-input'
import { Label } from '@/components/ui/label'
import { SectionHeading } from '@/components/ui/section-heading'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'

/**
 * Renders the shared automatic video-sync controls.
 *
 * @param {object} props Video sync state and actions.
 * @returns {JSX.Element|null} Rendered controls, or null without activity data.
 */
export function VideoSyncControls({
  activitySummary,
  canResetCreationTime,
  filenameCreationTimeAvailable,
  importedVideoTimeSource,
  offsetInput,
  timezone,
  videoSyncTimezoneMode,
  videoSyncWarning,
  openManualVideoSync,
  computeVideoSync,
  incrementOffset,
  resetVideoCreationTime,
  setOffsetInput,
  setVideoCreationTimeFromFilename,
  setVideoSyncTimezoneMode,
  submitOffsetInput,
}) {
  const { t } = useTranslation()
  if (!activitySummary) return null

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between">
        <SectionHeading icon={Clock3} title={t('toolbar.videoSync', 'Video Sync')} variant="drawer" />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="ml-2 h-6 w-6 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
          disabled={!canResetCreationTime}
          onClick={resetVideoCreationTime}
          aria-label={t('toolbar.restoreDetectedVideoCreationTime', 'Restore detected video creation time')}
        >
          <RotateCcw className="h-3 w-3" />
        </Button>
      </div>

      <div className="space-y-3">
        <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('toolbar.syncOffset', 'Sync Offset')}</Label>
        <div className="grid grid-cols-2 items-center gap-4">
          <div className="relative flex-1">
            <BlurInput
              type="text"
              value={offsetInput}
              onChange={(event) => setOffsetInput(event.target.value)}
              onBlur={(event) => submitOffsetInput(event.target.value)}
              className="h-9 text-xs pr-11 w-full border border-border/70"
              placeholder={t('toolbar.secondsOrMmss', 'Seconds or MM:SS')}
            />
            <div className="absolute inset-y-1 right-1 flex w-5 flex-col overflow-hidden rounded border border-none bg-surface-strong">
              <button
                type="button"
                aria-label={t('toolbar.increaseSyncOffset', 'Increase sync offset')}
                className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => incrementOffset(0.1)}
              >
                <ChevronUp className="h-3 w-3" />
              </button>
              <div className="h-px bg-border/60" />
              <button
                type="button"
                aria-label={t('toolbar.decreaseSyncOffset', 'Decrease sync offset')}
                className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => incrementOffset(-0.1)}
              >
                <ChevronDown className="h-3 w-3" />
              </button>
            </div>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-9 border-border/80 bg-surface-elevated px-3 text-xs font-semibold text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
            disabled={!activitySummary}
            onClick={() => computeVideoSync(activitySummary)}
            aria-keyshortcuts="Mod+Shift+A"
          >
            {t('toolbar.autosync', 'Auto-sync')}
          </Button>
        </div>
        <div className="grid grid-cols-2 gap-x-4 gap-y-4 pt-1">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold">{t('toolbar.creationTime', 'Creation Time')}</Label>
          <Tabs
            value={canResetCreationTime ? 'filename' : 'detected'}
            onValueChange={(value) => (value === 'filename' ? setVideoCreationTimeFromFilename() : resetVideoCreationTime())}
          >
            <TabsList variant="toolbar" className="grid h-8 w-full grid-cols-2 p-0.5">
              <TabsTrigger variant="toolbar" value="detected" className="h-full px-2 text-[0.7rem]">
                {t('toolbar.detected', 'Detected')}
              </TabsTrigger>
              <TabsTrigger variant="toolbar" value="filename" className="h-full px-2 text-[0.7rem]" disabled={!filenameCreationTimeAvailable}>
                {t('toolbar.filename', 'Filename')}
              </TabsTrigger>
            </TabsList>
          </Tabs>
          {timezone ? (
            <>
              <Label htmlFor="video-sync-timezone-toggle" className="mb-2 text-[10px] text-muted-foreground uppercase font-bold">
                {t('toolbar.applyTimezone', 'Apply Timezone')}
              </Label>
              <div className="mb-2 flex items-center gap-2">
                <Switch
                  id="video-sync-timezone-toggle"
                  checked={videoSyncTimezoneMode === 'utc'}
                  disabled={importedVideoTimeSource === 'filename'}
                  onCheckedChange={(checked) => setVideoSyncTimezoneMode(checked ? 'utc' : 'local')}
                  aria-label={t('toolbar.applyTimezone', 'Apply Timezone')}
                />
              </div>
            </>
          ) : null}
        </div>
      </div>
      {videoSyncWarning ? (
        <div className="flex gap-2 rounded-sm bg-amber-500/15 p-2 pl-4 text-amber-400 items-center justify-center">
          <Bell className="h-3 w-3 shrink-0" />
          <div className="min-w-0 flex-1 space-y-3 pl-2">
            <p className="text-[0.65rem] font-semibold leading-tight pl-1 pt-2">{videoSyncWarning}</p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-8 px-2 w-full text-[0.75rem] border-amber-400/30 bg-amber-400/10 text-amber-400 hover:bg-amber-500/20 hover:text-amber-200"
              onClick={openManualVideoSync}
            >
              {t('videoSync.openManualSync', 'Open Manual Sync')}
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  )
}

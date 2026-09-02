import { useMemo } from 'react'
import { Film, Trash2, Clock3, Bell, ChevronUp, ChevronDown, RotateCcw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { BlurInput } from '@/components/ui/blur-input'
import { SectionHeading } from '@/components/ui/section-heading'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useFileDropZone } from '../hooks/useFileDropZone'
import { FileDragCursor, FileDropZone } from './FileDropZone'
import { useTranslation } from 'react-i18next'
import { buildVideoDrawerViewModel } from '../utils/videoDrawerUtils'

function VideoSyncControls({
  activitySummary,
  canResetCreationTime,
  filenameCreationTimeAvailable,
  importedVideoTimeSource,
  offsetInput,
  timezone,
  videoSyncTimezoneMode,
  videoSyncWarning,
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
              onChange={(e) => setOffsetInput(e.target.value)}
              onBlur={(e) => submitOffsetInput(e.target.value)}
              className="h-9 text-xs pr-11 w-full border border-border/70"
              placeholder={t('toolbar.secondsOrMmss', 'Seconds or MM:SS')}
            />
            <div className="absolute inset-y-1 right-1 flex w-5 flex-col overflow-hidden rounded border border-none bg-surface-strong">
              <button
                type="button"
                aria-label={t('toolbar.increaseSyncOffset', 'Increase sync offset')}
                className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => incrementOffset(0.1)}
              >
                <ChevronUp className="h-3 w-3" />
              </button>
              <div className="h-px bg-border/60" />
              <button
                type="button"
                aria-label={t('toolbar.decreaseSyncOffset', 'Decrease sync offset')}
                className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                onMouseDown={(e) => e.preventDefault()}
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
          {timezone && (
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
          )}
        </div>
      </div>
      {videoSyncWarning && (
        <div className="flex gap-2 items-center rounded-sm bg-amber-500/15 p-2 pl-4 text-amber-400">
          <Bell className="h-3 w-3 shrink-0" />
          <p className="text-[0.65rem] font-semibold leading-tight">{videoSyncWarning}</p>
        </div>
      )}
    </section>
  )
}

/**
 * Renders the video import controls, imported video details, and sync settings.
 *
 * @param {object} props - Component props.
 * @param {object|null} props.videoSummary - Imported video metadata summary.
 * @param {() => void} props.onBrowseVideo - Opens the video file picker.
 * @param {(() => void)|null} props.onDeleteVideo - Clears the imported video.
 * @param {(selections: Array<File|string>) => void} props.onDropVideoFiles - Imports dropped video files or native paths.
 * @param {object} props.videoSync - Video sync state and handlers.
 * @returns {JSX.Element} Rendered drawer content.
 */
export function VideoDrawerContent({ videoSummary, onBrowseVideo, onDeleteVideo, onDropVideoFiles, videoSync }) {
  const { t } = useTranslation()
  const { dragPosition, dropZoneProps, dropZoneRef, isDraggingFile, isOverDropZone } = useFileDropZone(onDropVideoFiles)
  const drawerViewModel = useMemo(
    () => (videoSummary?.path ? buildVideoDrawerViewModel(videoSummary, videoSync.timezone, videoSync.videoSyncTimezoneMode, t) : null),
    [t, videoSummary, videoSync.timezone, videoSync.videoSyncTimezoneMode],
  )
  const displayFilename = videoSummary?.filename ?? videoSummary?.path?.split(/[/\\]/).pop()

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pb-4 thin-scrollbar">
      <FileDragCursor position={isDraggingFile ? dragPosition : null} />

      <Button type="button" className="h-9 w-full gap-2" onClick={onBrowseVideo} aria-keyshortcuts="Mod+I">
        <Film className="h-4 w-4" />
        {t('toolbar.loadVideo', 'Load video')}
      </Button>

      <FileDropZone
        dropZoneRef={dropZoneRef}
        dropZoneProps={dropZoneProps}
        isOverDropZone={isOverDropZone}
        label={t('toolbar.dropVideoFile', 'Drop video file')}
        sublabel="MP4, MOV, MKV"
      />

      {drawerViewModel ? (
        <div className="mt-10 space-y-8 border-t border-border/80 pt-2">
          <section className="space-y-4">
            <div className="flex items-center justify-between pb-2 pl-1 pt-4 text-sm font-extrabold text-foreground">
              <div className="flex min-w-0 items-center gap-2">
                <span className="min-w-0 truncate" title={displayFilename}>
                  {displayFilename}
                </span>
              </div>
              {onDeleteVideo ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 shrink-0 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
                  onClick={onDeleteVideo}
                  aria-label={t('toolbar.deleteVideo', 'Delete video')}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              ) : null}
            </div>
            <SectionHeading icon={Film} title={t('toolbar.details', 'Details')} variant="drawer" />
            <dl className="grid grid-cols-2 gap-x-4.5 gap-y-1.5 px-2 text-xs">
              {drawerViewModel.metadataRows.map((row) => (
                <div key={row.label} className="contents">
                  <dt className="font-bold text-muted-foreground">{row.label}</dt>
                  <dd className="min-w-0 wrap-break-words text-left font-medium text-foreground/90" title={row.value}>
                    {row.value}
                  </dd>
                </div>
              ))}
            </dl>
          </section>

          <VideoSyncControls {...videoSync} importedVideoTimeSource={videoSummary?.timeSource ?? null} />
        </div>
      ) : null}
    </div>
  )
}

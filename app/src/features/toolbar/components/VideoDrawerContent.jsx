import { AlertTriangle, Film, Trash2 } from 'lucide-react'
import { BlurInput } from '@/components/ui/blur-input'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { SectionHeading } from '@/components/ui/section-heading'
import { VideoSyncControls } from '@/features/video-sync'
import { useFileDropZone } from '../hooks/useFileDropZone'
import { useVideoClipRows } from '../hooks/useVideoClipRows'
import { FileDragCursor, FileDropZone } from './FileDropZone'
import { useTranslation } from 'react-i18next'

function VideoClipRow({ row }) {
  const { t } = useTranslation()
  const offsetInputId = `video-offset-${row.id}`

  return (
    <section className="space-y-4" aria-label={row.filename}>
      <div className="flex min-w-0 items-center gap-2 pb-2 pl-1 pt-4 text-sm font-extrabold text-foreground">
        <span className="min-w-0 flex-1 truncate" title={row.filename}>
          {row.filename}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
          onClick={row.onRemove}
          aria-label={t('toolbar.removeVideo', { defaultValue: 'Remove {{name}}', name: row.filename })}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
      {row.warnings.length > 0 && (
        <ul className="space-y-1 px-2 text-xs text-destructive">
          {row.warnings.map((warning) => (
            <li key={warning} className="flex items-start gap-1.5">
              <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" aria-hidden="true" />
              <span>{warning}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="grid grid-cols-2 items-center gap-4 px-2">
        <Label htmlFor={offsetInputId} className="text-xs font-bold text-muted-foreground">
          {t('toolbar.videoStart', 'Start on timeline')}
        </Label>
        <BlurInput
          id={offsetInputId}
          type="text"
          value={row.offsetInput}
          onChange={(event) => row.onOffsetCommit(event.target.value)}
          className="h-8 text-xs"
          placeholder={t('toolbar.secondsOrMmss', 'Seconds or MM:SS')}
        />
      </div>
      <SectionHeading icon={Film} title={t('toolbar.details', 'Details')} variant="drawer" />
      <dl className="grid grid-cols-2 gap-x-4.5 gap-y-1.5 px-2 text-xs">
        {row.metadataRows.map((metadataRow) => (
          <div key={metadataRow.label} className="contents">
            <dt className="font-bold text-muted-foreground">{metadataRow.label}</dt>
            <dd className="min-w-0 wrap-break-words text-left font-medium text-foreground/90" title={metadataRow.value}>
              {metadataRow.value}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  )
}

/**
 * Renders the video import controls, one row per imported clip, and the session sync settings.
 *
 * @param {object} props - Component props.
 * @param {Array<object>} props.videoSummaries - Imported video metadata summaries in timeline order.
 * @param {() => void} props.onBrowseVideo - Opens the video file picker.
 * @param {() => void} props.onDeleteVideo - Removes every imported video.
 * @param {(videoId: string) => void} props.onRemoveVideo - Removes one imported video.
 * @param {(selections: Array<File|string>) => void} props.onDropVideoFiles - Imports dropped video files or native paths.
 * @param {object} props.videoSync - Video sync state and handlers.
 * @returns {JSX.Element} Rendered drawer content.
 */
export function VideoDrawerContent({ videoSummaries, onBrowseVideo, onDeleteVideo, onRemoveVideo, onDropVideoFiles, videoSync }) {
  const { t } = useTranslation()
  const { dragPosition, dropZoneProps, dropZoneRef, isDraggingFile, isOverDropZone } = useFileDropZone(onDropVideoFiles)
  const rows = useVideoClipRows({
    videoSummaries,
    timezone: videoSync.timezone,
    videoSyncTimezoneMode: videoSync.videoSyncTimezoneMode,
    onRemoveVideo,
  })

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pb-4 thin-scrollbar">
      <FileDragCursor position={isDraggingFile ? dragPosition : null} />

      <Button type="button" className="h-9 w-full gap-2" onClick={onBrowseVideo} aria-keyshortcuts="Mod+I">
        <Film className="h-4 w-4" />
        {t('toolbar.loadVideo', 'Load videos')}
      </Button>

      <FileDropZone
        dropZoneRef={dropZoneRef}
        dropZoneProps={dropZoneProps}
        isOverDropZone={isOverDropZone}
        label={t('toolbar.dropVideoFile', 'Drop video files')}
        sublabel="MP4, MOV, MKV"
      />

      {rows.length > 0 ? (
        <div className="mt-10 space-y-8 border-t border-border/80 pt-2">
          <div className="flex justify-end pt-2">
            <Button
              type="button"
              variant="ghost"
              className="h-7 gap-1.5 px-2 text-xs text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
              onClick={onDeleteVideo}
            >
              <Trash2 className="h-3.5 w-3.5" />
              {t('toolbar.deleteVideo', 'Remove all videos')}
            </Button>
          </div>
          {rows.map((row) => (
            <VideoClipRow key={row.id} row={row} />
          ))}

          <VideoSyncControls {...videoSync} />
        </div>
      ) : null}
    </div>
  )
}

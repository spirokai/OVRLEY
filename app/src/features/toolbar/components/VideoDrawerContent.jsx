import { useMemo } from 'react'
import { Film, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { SectionHeading } from '@/components/ui/section-heading'
import { VideoSyncControls } from '@/features/video-sync'
import { useFileDropZone } from '../hooks/useFileDropZone'
import { FileDragCursor, FileDropZone } from './FileDropZone'
import { useTranslation } from 'react-i18next'
import { buildVideoDrawerViewModel } from '../utils/videoDrawerUtils'

function VideoDetails({ videoSummary, videoSync }) {
  const { t } = useTranslation()
  const drawerViewModel = useMemo(
    () => buildVideoDrawerViewModel(videoSummary, videoSync.timezone, videoSync.videoSyncTimezoneMode, t),
    [t, videoSummary, videoSync.timezone, videoSync.videoSyncTimezoneMode],
  )

  return (
    <section className="space-y-4">
      <div className="flex min-w-0 items-center gap-2 pb-2 pl-1 pt-4 text-sm font-extrabold text-foreground">
        <span className="min-w-0 truncate" title={videoSummary.filename}>
          {videoSummary.filename}
        </span>
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
  )
}

/**
 * Renders the video import controls, imported video details, and sync settings.
 *
 * @param {object} props - Component props.
 * @param {Array<object>} props.videoSummaries - Imported video metadata summaries in timeline order.
 * @param {() => void} props.onBrowseVideo - Opens the video file picker.
 * @param {() => void} props.onDeleteVideo - Clears the imported videos.
 * @param {(selections: Array<File|string>) => void} props.onDropVideoFiles - Imports dropped video files or native paths.
 * @param {object} props.videoSync - Video sync state and handlers.
 * @returns {JSX.Element} Rendered drawer content.
 */
export function VideoDrawerContent({ videoSummaries, onBrowseVideo, onDeleteVideo, onDropVideoFiles, videoSync }) {
  const { t } = useTranslation()
  const { dragPosition, dropZoneProps, dropZoneRef, isDraggingFile, isOverDropZone } = useFileDropZone(onDropVideoFiles)

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

      {videoSummaries.length > 0 ? (
        <div className="mt-10 space-y-8 border-t border-border/80 pt-2">
          <div className="flex justify-end pt-2">
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
          </div>
          {videoSummaries.map((videoSummary) => (
            <VideoDetails key={videoSummary.id} videoSummary={videoSummary} videoSync={videoSync} />
          ))}

          <VideoSyncControls {...videoSync} />
        </div>
      ) : null}
    </div>
  )
}

import { AlertTriangle, Blend, Film, Scissors, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { SecondsInput } from '@/components/ui/seconds-input'
import { SectionHeading } from '@/components/ui/section-heading'
import { Switch } from '@/components/ui/switch'
import { JoinTransitionControl, TransitionSelect } from '@/features/transitions'
import { VideoSyncControls } from '@/features/video-sync'
import { useFileDropZone } from '../hooks/useFileDropZone'
import { useTransitionDefaultsSection } from '../hooks/useTransitionDefaultsSection'
import { useVideoClipRows } from '../hooks/useVideoClipRows'
import { FileDragCursor, FileDropZone } from './FileDropZone'
import { useTranslation } from 'react-i18next'

function VideoSegmentRow({ segment, clipId, filename, showRange }) {
  const { t } = useTranslation()
  const includedLabel = t('toolbar.includeInExport', 'Include in export')
  const transitionId = `video-segment-transition-${clipId}-${segment.key}`
  return (
    <li className={`space-y-2 ${showRange ? 'rounded-xs border border-border/50 p-2' : ''}`}>
      {showRange ? (
        <div className="flex items-center justify-between gap-2 text-xs font-bold tabular-nums text-muted-foreground">
          <button
            type="button"
            className="underline-offset-2 hover:text-foreground hover:underline"
            title={t('toolbar.goToPart', 'Go to part')}
            onClick={segment.onSeek}
          >
            {segment.startLabel} – {segment.endLabel}
          </button>
          {segment.onRemoveCut !== null ? (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-6 w-6 shrink-0 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
              onClick={segment.onRemoveCut}
              aria-label={t('toolbar.removeCutAt', { defaultValue: 'Remove the cut at {{time}}', time: segment.startLabel })}
            >
              <Scissors className="h-3 w-3" />
            </Button>
          ) : null}
        </div>
      ) : null}
      <div className="grid grid-cols-2 items-center gap-4">
        <span className="text-xs font-bold text-muted-foreground">{includedLabel}</span>
        <Switch
          aria-label={`${includedLabel}: ${filename} ${segment.startLabel}`}
          checked={segment.included}
          onCheckedChange={segment.onIncludedChange}
        />
      </div>
      {segment.transition !== null ? (
        <div className="space-y-1">
          <Label htmlFor={transitionId} className="text-xs font-bold text-muted-foreground">
            {t('toolbar.transitionIn', 'Transition in')}
          </Label>
          <JoinTransitionControl id={transitionId} model={segment.transition} />
        </div>
      ) : null}
    </li>
  )
}

function TransitionDefaultsSection() {
  const { t } = useTranslation()
  const section = useTransitionDefaultsSection()
  return (
    <section className="space-y-3" aria-label={t('toolbar.transitionDefaults', 'Default transitions')}>
      <SectionHeading icon={Blend} title={t('toolbar.transitionDefaults', 'Default transitions')} variant="drawer" />
      <p className="px-2 text-[0.7rem] text-muted-foreground">
        {t(
          'toolbar.transitionDefaultsHint',
          'Used at every join you have not set yourself. Videos and parts closer together than the continuity threshold are joined without a transition.',
        )}
      </p>
      <div className="grid grid-cols-2 items-center gap-x-4 gap-y-2 px-2">
        <Label htmlFor="transition-defaults-type" className="text-xs font-bold text-muted-foreground">
          {t('video-preview.transitionType', 'Transition')}
        </Label>
        <TransitionSelect
          id="transition-defaults-type"
          value={section.type}
          label={section.typeLabel}
          previewType={section.previewType}
          onSelect={section.onTypeChange}
        />
        <Label htmlFor="transition-defaults-duration" className="text-xs font-bold text-muted-foreground">
          {t('video-preview.transitionDuration', 'Duration')}
        </Label>
        <SecondsInput
          id="transition-defaults-duration"
          value={section.durationSeconds}
          fps={section.durationFps}
          mode="duration"
          onCommit={section.onDurationCommit}
          className="h-7 text-xs"
        />
        <Label htmlFor="transition-defaults-threshold" className="text-xs font-bold text-muted-foreground">
          {t('video-preview.continuityThreshold', 'Continuity threshold')}
        </Label>
        <SecondsInput
          id="transition-defaults-threshold"
          value={section.thresholdSeconds}
          fps={null}
          onCommit={section.onThresholdCommit}
          className="h-7 text-xs"
        />
      </div>
    </section>
  )
}

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
          aria-label={t('toolbar.removeVideo', { defaultValue: 'Remove {{name}} from project', name: row.filename })}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
      {row.warnings.length > 0 && (
        <ul className="space-y-1 px-2 text-xs text-destructive">
          {/* Warnings may read alike: two overlapping videos can share a filename. */}
          {row.warnings.map((warning, index) => (
            <li key={index} className="flex items-start gap-1.5">
              <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" aria-hidden="true" />
              <span>{warning}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="grid grid-cols-2 items-start gap-4 px-2">
        <Label htmlFor={offsetInputId} className="flex min-h-8 items-center text-xs font-bold text-muted-foreground">
          {t('toolbar.videoStart', 'Start on timeline')}
        </Label>
        <SecondsInput
          id={offsetInputId}
          value={row.offsetSeconds}
          fps={row.offsetFps}
          signed
          onCommit={row.onOffsetCommit}
          reset={{
            isManual: row.offsetIsManual,
            onReset: row.onOffsetReset,
            label: t('toolbar.resetVideoStart', { defaultValue: 'Place {{name}} automatically again', name: row.filename }),
            manualHint: t('toolbar.resetVideoStartHint', 'Set by hand. Click to place it by its recording time again.'),
            autoHint: t('toolbar.videoStartAutoHint', 'Placed automatically. Type a time to set it by hand.'),
          }}
          className="h-8 text-xs"
        />
      </div>
      {row.segments.length > 1 ? <SectionHeading icon={Scissors} title={t('toolbar.videoParts', 'Parts')} variant="drawer" /> : null}
      <ul className="space-y-2 px-2">
        {row.segments.map((segment) => (
          <VideoSegmentRow key={segment.key} segment={segment} clipId={row.id} filename={row.filename} showRange={row.segments.length > 1} />
        ))}
      </ul>
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
 * @param {() => void} props.onBrowseVideo - Opens the video file picker.
 * @param {() => void} props.onDeleteVideo - Removes every imported video.
 * @param {(videoId: string) => void} props.onRemoveVideo - Removes one imported video.
 * @param {(selections: Array<File|string>) => void} props.onDropVideoFiles - Imports dropped video files or native paths.
 * @param {object} props.videoSync - Video sync state and handlers.
 * @returns {JSX.Element} Rendered drawer content.
 */
export function VideoDrawerContent({ onBrowseVideo, onDeleteVideo, onRemoveVideo, onDropVideoFiles, videoSync }) {
  const { t } = useTranslation()
  const { dragPosition, dropZoneProps, dropZoneRef, isDraggingFile, isOverDropZone } = useFileDropZone(onDropVideoFiles)
  const rows = useVideoClipRows({
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
          {videoSync.activitySummary ? <VideoSyncControls {...videoSync} /> : null}

          {rows.map((row) => (
            <VideoClipRow key={row.id} row={row} />
          ))}

          <TransitionDefaultsSection />
        </div>
      ) : null}
    </div>
  )
}

import { AlertTriangle, Blend, Film, RotateCcw, Scissors, Trash2 } from 'lucide-react'
import { BlurInput } from '@/components/ui/blur-input'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { SecondsInput } from '@/components/ui/seconds-input'
import { SectionHeading } from '@/components/ui/section-heading'
import { JoinTransitionControl, TransitionSelect } from '@/features/transitions'
import { VideoSyncControls } from '@/features/video-sync'
import { useFileDropZone } from '../hooks/useFileDropZone'
import { useTransitionDefaultsSection } from '../hooks/useTransitionDefaultsSection'
import { useVideoClipRows } from '../hooks/useVideoClipRows'
import { FileDragCursor, FileDropZone } from './FileDropZone'
import { useTranslation } from 'react-i18next'

function DeletedPartRow({ part, filename }) {
  const { t } = useTranslation()
  const id = `deleted-part-${part.id}`
  return (
    <li
      className="space-y-2 rounded-xs border border-border/50 p-2"
      aria-label={t('toolbar.deletedPartOf', { defaultValue: 'Deleted part of {{name}}', name: filename })}
    >
      <div className="grid grid-cols-[auto_minmax(0,1fr)_auto_minmax(0,1fr)_auto] items-center gap-2">
        <Label htmlFor={`${id}-from`} className="text-xs font-bold text-muted-foreground">
          {t('toolbar.deletedFrom', 'From')}
        </Label>
        <BlurInput
          id={`${id}-from`}
          type="text"
          value={part.fromInput}
          onChange={(event) => part.onFromCommit(event.target.value)}
          className="h-7 text-xs"
        />
        <Label htmlFor={`${id}-to`} className="text-xs font-bold text-muted-foreground">
          {t('toolbar.deletedTo', 'To')}
        </Label>
        <BlurInput
          id={`${id}-to`}
          type="text"
          value={part.toInput}
          onChange={(event) => part.onToCommit(event.target.value)}
          className="h-7 text-xs"
        />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
          onClick={part.onRemove}
          aria-label={t('toolbar.restoreDeletedPart', { defaultValue: 'Restore {{from}}–{{to}}', from: part.fromInput, to: part.toInput })}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
      <div className="grid grid-cols-[auto_minmax(0,1fr)] items-start gap-2">
        <Label htmlFor={`${id}-transition`} className="pt-1.5 text-xs font-bold text-muted-foreground">
          {t('video-preview.transitionType', 'Transition')}
        </Label>
        <JoinTransitionControl id={`${id}-transition`} model={part.transition} />
      </div>
      <div className="flex items-center justify-between text-[0.65rem] text-muted-foreground">
        <button type="button" className="underline-offset-2 hover:text-foreground hover:underline" onClick={part.onSeek}>
          {t('toolbar.goToDeletedPart', 'Go to part')}
        </button>
        <span className="tabular-nums">
          {part.transition.isAvailable
            ? t('toolbar.deletedLength', { defaultValue: '{{duration}} removed', duration: part.durationLabel })
            : t('toolbar.deletedPartWithoutJoin', 'At a clip edge: nothing to join')}
        </span>
      </div>
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
        <div className="flex items-start gap-1">
          <div className="min-w-0 flex-1">
            <SecondsInput
              id={offsetInputId}
              value={row.offsetSeconds}
              fps={row.offsetFps}
              signed
              onCommit={row.onOffsetCommit}
              className="h-8 text-xs"
            />
          </div>
          {row.offsetIsManual ? (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
              onClick={row.onOffsetReset}
              aria-label={t('toolbar.resetVideoStart', { defaultValue: 'Place {{name}} automatically again', name: row.filename })}
              title={t('toolbar.resetVideoStartHint', 'Set by hand. Click to place it by its recording time again.')}
            >
              <RotateCcw className="h-3.5 w-3.5" />
            </Button>
          ) : null}
        </div>
      </div>
      {row.transitionIn !== null ? (
        <div className="grid grid-cols-2 items-start gap-4 px-2">
          <Label htmlFor={`video-transition-${row.id}`} className="pt-1.5 text-xs font-bold text-muted-foreground">
            {t('toolbar.transitionIn', 'Transition into this video')}
          </Label>
          <JoinTransitionControl id={`video-transition-${row.id}`} model={row.transitionIn} />
        </div>
      ) : null}
      {row.deletedParts.length > 0 ? (
        <>
          <SectionHeading icon={Scissors} title={t('toolbar.deletedParts', 'Deleted parts')} variant="drawer" />
          <ul className="space-y-2 px-2">
            {row.deletedParts.map((part) => (
              <DeletedPartRow key={part.id} part={part} filename={row.filename} />
            ))}
          </ul>
        </>
      ) : null}
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

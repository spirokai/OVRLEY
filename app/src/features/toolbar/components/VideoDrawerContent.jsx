import { Fragment } from 'react'
import { AlertTriangle, Clock3, Film, FolderSearch, Gauge, RefreshCw, Scissors, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { SecondsInput } from '@/components/ui/seconds-input'
import { SectionHeading } from '@/components/ui/section-heading'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Toggle } from '@/components/ui/toggle'
import { JoinTransitionControl } from '@/features/transitions'
import { cn } from '@/lib/utils'
import { useFileDropZone } from '../hooks/useFileDropZone'
import { useSelectedVideoRow } from '../hooks/useSelectedVideoRow'
import { FileDragCursor, FileDropZone } from './FileDropZone'
import { useTranslation } from 'react-i18next'

/** Text of a part switched off for export, keyed to its card's `data-included`. */
const EXCLUDED_PART_TEXT =
  'group-data-[included=false]:text-muted-foreground group-data-[included=false]:opacity-35 group-data-[included=false]:line-through'

/** A transition between a part and what follows it, on a dashed line; its label is for screen readers only. */
function SegmentJoin({ transition, id, label }) {
  return (
    <li className="relative min-w-0 pt-3 pb-2 pl-7 before:absolute before:top-1 before:bottom-0 before:left-3 before:border-l before:border-dashed before:border-foreground">
      <Label htmlFor={id} className="sr-only">
        {label}
      </Label>
      <JoinTransitionControl id={id} model={transition} />
    </li>
  )
}

/**
 * One part of a video. The switch includes the part in the export and leads the row; the icon toggle
 * before it shows the activity widgets over the part, in the preview and the render.
 */
function SegmentCard({ segment, filename }) {
  const { t } = useTranslation()
  const includedLabel = t('toolbar.includeInExport', 'Include in export')
  const overlayLabel = t('toolbar.showOverlay', 'Show overlay')
  const hideOverlayLabel = t('toolbar.hideOverlay', 'Hide overlay')
  const removeCutLabel = t('toolbar.removeCutAt', { defaultValue: 'Remove the cut at {{time}}', time: segment.startLabel })
  return (
    <li
      data-included={segment.included}
      className="group mt-1 flex min-w-0 items-center gap-2 rounded-sm bg-foreground/6 py-1.5 pl-2.5 pr-2 first:mt-0 data-[included=false]:bg-foreground/3"
    >
      <SimpleTooltip content={t('toolbar.goToPart', 'Go to part')}>
        <button
          type="button"
          className={cn('text-xs font-bold tabular-nums text-foreground underline-offset-2 hover:underline', EXCLUDED_PART_TEXT)}
          onClick={segment.onSeek}
        >
          {segment.startLabel} – {segment.endLabel}
        </button>
      </SimpleTooltip>
      {segment.onRemoveCut !== null ? (
        <SimpleTooltip content={removeCutLabel}>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-muted-foreground/60 transition-colors hover:bg-surface-accent-soft hover:text-primary"
            onClick={segment.onRemoveCut}
            aria-label={removeCutLabel}
          >
            <Trash2 className="size-4" />
          </Button>
        </SimpleTooltip>
      ) : null}
      <SimpleTooltip content={segment.showsOverlay ? hideOverlayLabel : overlayLabel} className="ml-auto">
        <Toggle
          size="sm"
          className="h-7 min-w-7 cursor-pointer rounded-sm bg-foreground/10 px-0 text-muted-foreground hover:bg-foreground/15 hover:text-foreground data-[state=on]:bg-primary data-[state=on]:text-primary-foreground data-[state=on]:hover:bg-primary/90 data-[state=on]:hover:text-primary-foreground"
          pressed={segment.showsOverlay}
          onPressedChange={segment.onShowsOverlayChange}
          aria-label={`${overlayLabel}: ${filename} ${segment.startLabel}`}
        >
          <Gauge className="size-4.5" />
        </Toggle>
      </SimpleTooltip>
      <Switch
        className="ml-1"
        aria-label={`${includedLabel}: ${filename} ${segment.startLabel}`}
        checked={segment.included}
        onCheckedChange={segment.onIncludedChange}
      />
    </li>
  )
}

/** Where the last transition leads: the video that follows this one on the timeline. */
function NextClipCard({ filename }) {
  const { t } = useTranslation()
  return (
    <li className="flex min-w-0 items-center gap-2 rounded-sm border border-dashed border-foreground/15 py-1.5 pl-2.5 pr-2">
      <span className="shrink-0 text-[0.7rem] font-bold uppercase tracking-wide text-muted-foreground/70">{t('toolbar.nextClip', 'Next clip')}</span>
      <SimpleTooltip content={filename} className="min-w-0" align="start" wrapText>
        <span className="min-w-0 truncate text-xs text-muted-foreground">{filename}</span>
      </SimpleTooltip>
    </li>
  )
}

function SegmentList({ segments, exits, clipId, filename }) {
  const { t } = useTranslation()
  return (
    <ul>
      {segments.map((segment) => (
        <Fragment key={segment.key}>
          {segment.transition !== null ? (
            <SegmentJoin
              transition={segment.transition}
              id={`video-segment-transition-${clipId}-${segment.key}`}
              label={t('toolbar.transitionIn', 'Transition in')}
            />
          ) : null}
          <SegmentCard segment={segment} filename={filename} />
        </Fragment>
      ))}
      {exits.map((exit) => (
        <Fragment key={exit.key}>
          <SegmentJoin transition={exit.transition} id={`video-transition-${exit.key}`} label={exit.label} />
          <NextClipCard filename={exit.nextFilename} />
        </Fragment>
      ))}
    </ul>
  )
}

/** Field label of the drawer's two-column rows; `pl-0` drops the label primitive's padding so every label starts at the grid's edge. */
const FIELD_LABEL = 'flex min-h-8 items-center pl-0 text-xs font-bold text-muted-foreground'

/** Sync of one video: its start on the timeline, automatic sync, and how its recording time is read. */
function VideoSyncSection({ row }) {
  const { t } = useTranslation()
  const offsetInputId = `video-offset-${row.id}`
  const timezoneInputId = `video-timezone-${row.id}`
  return (
    <section className="space-y-3" aria-label={t('toolbar.syncTheVideo', 'Sync the video')}>
      <SectionHeading icon={Clock3} title={t('toolbar.syncTheVideo', 'Sync the video')} variant="drawer" />
      <div className="grid grid-cols-2 items-start gap-x-4.5 gap-y-3 px-2">
        <Label htmlFor={offsetInputId} className={FIELD_LABEL}>
          {t('toolbar.videoStart', 'Start on timeline')}
        </Label>
        <SecondsInput id={offsetInputId} value={row.offsetSeconds} fps={row.offsetFps} signed onCommit={row.onOffsetCommit} className="h-8 text-xs" />
        <span className={FIELD_LABEL}>{t('toolbar.creationTime', 'Creation Time')}</span>
        <Tabs value={row.creationTimeSource} onValueChange={row.onCreationTimeSourceChange}>
          <TabsList variant="toolbar" className="grid h-8 w-full grid-cols-2 p-0.5">
            <TabsTrigger variant="toolbar" value="detected" className="h-full px-2 text-[0.7rem]">
              {t('toolbar.detected', 'Detected')}
            </TabsTrigger>
            <TabsTrigger variant="toolbar" value="filename" className="h-full px-2 text-[0.7rem]" disabled={!row.filenameCreationTimeAvailable}>
              {t('toolbar.filename', 'Filename')}
            </TabsTrigger>
          </TabsList>
        </Tabs>
        {row.timezone !== null ? (
          <>
            <Label htmlFor={timezoneInputId} className={FIELD_LABEL}>
              {t('toolbar.applyTimezone', 'Apply Timezone')}
            </Label>
            <div className="flex min-h-8 items-center">
              <Switch
                id={timezoneInputId}
                checked={row.appliesTimezone}
                disabled={row.timezoneDisabled}
                onCheckedChange={row.onAppliesTimezoneChange}
              />
            </div>
          </>
        ) : null}
        <Button type="button" variant="secondary" className="col-span-2 w-full gap-2" onClick={row.onSync}>
          <RefreshCw className="size-4" /> {t('toolbar.syncTheVideo', 'Sync the video')}
        </Button>
      </div>
    </section>
  )
}

function SelectedVideoDetails({ row }) {
  const { t } = useTranslation()

  return (
    <section className="space-y-4" aria-label={row.filename}>
      <div className="flex min-w-0 items-center gap-2 pb-2 pl-1 pt-4 text-sm font-extrabold text-foreground">
        <SimpleTooltip content={row.filename} className="min-w-0 flex-1" wrapText>
          <span className="min-w-0 truncate">{row.filename}</span>
        </SimpleTooltip>
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
      {row.onLocate !== null ? (
        <div className="space-y-2 px-2">
          <p className="text-[0.7rem] text-muted-foreground">
            {t('toolbar.lastKnownLocation', 'Last location:')} <span className="break-all select-text">{row.path}</span>
          </p>
          <Button type="button" className="h-8 w-full gap-2" onClick={row.onLocate}>
            <FolderSearch className="h-4 w-4" />
            {t('toolbar.locateVideo', 'Locate video…')}
          </Button>
        </div>
      ) : null}
      <VideoSyncSection row={row} />
      <SectionHeading icon={Film} title={t('toolbar.details', 'Details')} variant="drawer" />
      <dl className="grid grid-cols-2 gap-x-4.5 gap-y-1.5 px-2 text-xs">
        {row.metadataRows.map((metadataRow) => (
          <div key={metadataRow.label} className="contents">
            <dt className="font-bold text-muted-foreground">{metadataRow.label}</dt>
            <dd className="min-w-0 wrap-break-words text-left font-medium text-foreground/90">{metadataRow.value}</dd>
          </div>
        ))}
      </dl>
      {/* A video without cuts is one part: the same card, without a removable cut. */}
      <SectionHeading icon={Scissors} title={t('toolbar.videoParts', 'Parts')} variant="drawer" />
      <SegmentList segments={row.segments} exits={row.exits} clipId={row.id} filename={row.filename} />
    </section>
  )
}

/**
 * Renders the video import controls and the details of the selected video clip,
 * including its sync; the video panel holds the video list and the default transitions.
 *
 * @param {object} props - Component props.
 * @param {boolean} props.hasVideos - Whether the project has videos.
 * @param {() => void} props.onBrowseVideo - Opens the video file picker.
 * @param {(videoId: string) => void} props.onRemoveVideo - Removes one imported video.
 * @param {(videoId: string) => void} props.onLocateVideo - Asks for the file of a missing video.
 * @param {(selections: Array<File|string>) => void} props.onDropVideoFiles - Imports dropped video files or native paths.
 * @returns {JSX.Element} Rendered drawer content.
 */
export function VideoDrawerContent({ hasVideos, onBrowseVideo, onRemoveVideo, onLocateVideo, onDropVideoFiles }) {
  const { t } = useTranslation()
  const { dragPosition, dropZoneProps, dropZoneRef, isDraggingFile, isOverDropZone } = useFileDropZone(onDropVideoFiles)
  const row = useSelectedVideoRow({ onRemoveVideo, onLocateVideo })

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pb-4 thin-scrollbar" data-keeps-clip-selection>
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

      {hasVideos ? (
        <div className="mt-10 space-y-8 border-t border-border/80 pt-6">
          {row !== null ? (
            <SelectedVideoDetails row={row} />
          ) : (
            <p className="px-2 text-center text-xs text-muted-foreground">{t('toolbar.selectVideoHint', 'Select a video to see its details.')}</p>
          )}
        </div>
      ) : null}
    </div>
  )
}

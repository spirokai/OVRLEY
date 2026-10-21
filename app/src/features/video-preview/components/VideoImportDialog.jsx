/**
 * Review of a video import before anything joins the collection: every video in
 * timeline order with its sync, whether it will be imported, overlaps, and what
 * prevents a single-file export, plus the sync and transition settings.
 * Pure presentational - all logic is in useVideoImportReview.
 */

import { AlertTriangle, ArrowDown, ArrowUp, Blend, Download, Loader2, RotateCcw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import { SecondsInput } from '@/components/ui/seconds-input'
import { SectionHeading } from '@/components/ui/section-heading'
import { TransitionSelect } from '@/features/transitions'
import { VideoSyncControls } from '@/features/video-sync'
import { formatSecondsClock } from '@/lib/time-format'
import { cn } from '@/lib/utils'

const ICON_BUTTON_CLASS = 'h-8 w-8 shrink-0 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary'

function verdictLabel(row, windowKind, t) {
  if (row.verdict === 'kept') return t('video-preview.importKept', 'Already imported')
  if (row.verdict === 'import') return t('video-preview.importJoins', 'Will be imported')
  if (row.verdict === 'outside') return t('video-preview.importKeptOutside', 'Starts more than an hour before or after the activity')
  if (windowKind === 'activity')
    return t('video-preview.importOutsideActivity', 'Not imported: starts more than an hour before or after the activity')
  if (windowKind === 'videos') return t('video-preview.importOutsideVideos', 'Not imported: starts more than an hour away from the other videos')
  return t('video-preview.importSpanTooLong', 'Not imported: the videos start more than 12 hours apart. Edit their sync to import them.')
}

function VideoImportRow({ row, windowKind }) {
  const { t } = useTranslation()
  const isExcluded = row.verdict === 'excluded'
  return (
    <li
      aria-label={row.filename}
      className="grid grid-cols-[minmax(0,1fr)_9.5rem_4.25rem] items-start gap-3 rounded-sm px-2 py-2 hover:bg-surface-elevated"
    >
      <div className={cn('min-w-0 space-y-0.5', isExcluded && 'opacity-60')}>
        <p className="flex min-w-0 items-center gap-2 text-xs font-medium text-foreground">
          <span className="truncate" title={row.path}>
            {row.filename}
          </span>
          {row.isNew ? (
            <Badge variant="secondary" className="shrink-0 px-1.5 py-0 text-[0.6rem]">
              {t('video-preview.importNew', 'New')}
            </Badge>
          ) : null}
        </p>
        <p className={cn('text-[0.7rem] tabular-nums', row.isIncompatible ? 'text-amber-500' : 'text-muted-foreground/70')}>{row.formatLabel}</p>
        <p
          className={cn(
            'text-[0.7rem] leading-tight',
            row.verdict === 'import' ? 'text-green-700' : row.verdict === 'kept' ? 'text-muted-foreground/70' : 'text-amber-500',
          )}
        >
          {verdictLabel(row, windowKind, t)}
        </p>
        {row.isPositioned ? (
          <p className="text-[0.7rem] leading-tight text-muted-foreground/70">
            {t('video-preview.importPositioned', 'Recording time unknown: use the arrows to place it after another video.')}
          </p>
        ) : null}
        {row.overlaps.map((overlap) => (
          <p key={overlap.name} className="text-[0.7rem] leading-tight text-amber-500">
            {t('video-preview.importOverlap', {
              defaultValue: 'Overlaps {{name}} by {{duration}}',
              name: overlap.name,
              duration: formatSecondsClock(overlap.seconds),
            })}
          </p>
        ))}
      </div>
      <SecondsInput
        value={row.offsetSeconds}
        fps={row.fps}
        signed
        onCommit={row.onOffsetCommit}
        className="h-8 text-xs"
        aria-label={t('video-preview.importSyncOf', { defaultValue: 'Sync of {{name}} (seconds)', name: row.filename })}
      />
      <div className="flex flex-wrap items-center justify-end">
        {row.isManual ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className={ICON_BUTTON_CLASS}
            onClick={row.onOffsetReset}
            aria-label={t('toolbar.resetVideoStart', { defaultValue: 'Place {{name}} automatically again', name: row.filename })}
            title={t('toolbar.resetVideoStartHint', 'Set by hand. Click to place it by its recording time again.')}
          >
            <RotateCcw className="h-3.5 w-3.5" />
          </Button>
        ) : null}
        {row.onMoveUp ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className={ICON_BUTTON_CLASS}
            onClick={row.onMoveUp}
            aria-label={t('video-preview.importMoveUp', { defaultValue: 'Move {{name}} up', name: row.filename })}
          >
            <ArrowUp className="h-3.5 w-3.5" />
          </Button>
        ) : null}
        {row.onMoveDown ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className={ICON_BUTTON_CLASS}
            onClick={row.onMoveDown}
            aria-label={t('video-preview.importMoveDown', { defaultValue: 'Move {{name}} down', name: row.filename })}
          >
            <ArrowDown className="h-3.5 w-3.5" />
          </Button>
        ) : null}
        {row.onRemove ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className={ICON_BUTTON_CLASS}
            onClick={row.onRemove}
            aria-label={t('video-preview.importRemoveFromProject', { defaultValue: 'Remove {{name}} from the project', name: row.filename })}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        ) : null}
      </div>
    </li>
  )
}

function TransitionSection({ transition }) {
  const { t } = useTranslation()
  return (
    <section className="space-y-3" aria-label={t('toolbar.transitionDefaults', 'Default transitions')}>
      <SectionHeading icon={Blend} title={t('toolbar.transitionDefaults', 'Default transitions')} variant="drawer" />
      <div className="grid grid-cols-2 items-center gap-x-4 gap-y-2">
        <Label htmlFor="import-transition-type" className="text-xs font-bold text-muted-foreground">
          {t('video-preview.transitionType', 'Transition')}
        </Label>
        <TransitionSelect
          id="import-transition-type"
          value={transition.type}
          label={transition.typeLabel}
          previewType={transition.previewType}
          onSelect={transition.onTypeChange}
        />
        <Label htmlFor="import-transition-duration" className="text-xs font-bold text-muted-foreground">
          {t('video-preview.transitionDuration', 'Duration (seconds)')}
        </Label>
        <SecondsInput
          id="import-transition-duration"
          value={transition.durationSeconds}
          fps={transition.durationFps}
          mode="duration"
          onCommit={transition.onDurationCommit}
          className="h-7 text-xs"
        />
      </div>
    </section>
  )
}

/**
 * Renders the import dialog.
 *
 * @param {object} props - Dialog model from `useVideoImportReview`.
 * @returns {JSX.Element} Rendered dialog.
 */
export function VideoImportDialog(props) {
  const { t } = useTranslation()
  const { open, phase, onCancel } = props
  const isProbing = phase === 'probing'
  const isImporting = phase === 'importing'
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && !isImporting && onCancel()}>
      <DialogContent
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
        className="w-full max-w-5xl rounded-sm border border-accent-border/80 bg-card p-6 shadow-2xl shadow-background/50"
      >
        <DialogTitle className="text-lg font-bold">{t('video-preview.importTitle', 'Import videos')}</DialogTitle>
        {isProbing || !open ? (
          <DialogDescription className="flex items-center gap-2 py-10 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('video-preview.importReading', { defaultValue: 'Reading {{count}} files…', count: props.fileCount })}
          </DialogDescription>
        ) : (
          <>
            <DialogDescription className="text-sm text-muted-foreground">
              {t(
                'video-preview.importDescription',
                'Check the sync of every video before importing. Sync is the start of a video in seconds from the start of the activity (or of the first video).',
              )}
            </DialogDescription>
            <div className="mt-4 grid gap-8 md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
              <div className="min-w-0 space-y-8">
                <VideoSyncControls {...props.sync} />
                <TransitionSection transition={props.transition} />
              </div>
              <div className="flex min-h-0 min-w-0 flex-col gap-3">
                {props.issues.length > 0 ? (
                  <div role="alert" className="flex gap-2 rounded-sm bg-amber-500/15 p-3 text-amber-500">
                    <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                    <div className="space-y-1 text-xs">
                      <p className="font-semibold">{t('video-preview.importSingleFileBlocked', 'These videos cannot be exported as one file:')}</p>
                      <ul className="space-y-1">
                        {props.issues.map((issue) => (
                          <li key={issue}>{issue}</li>
                        ))}
                      </ul>
                    </div>
                  </div>
                ) : null}
                <ul
                  aria-label={t('video-preview.importVideos', 'Videos')}
                  className="max-h-[55vh] min-h-0 space-y-1 overflow-y-auto rounded-sm border border-border/70 bg-surface p-2 [scrollbar-gutter:stable]"
                >
                  {props.rows.map((row) => (
                    <VideoImportRow key={row.id} row={row} windowKind={props.windowKind} />
                  ))}
                  {props.failures.map((failure) => (
                    <li key={failure.key} aria-label={failure.name} className="space-y-0.5 rounded-sm px-2 py-2 opacity-60">
                      <p className="truncate text-xs font-medium text-foreground" title={failure.key}>
                        {failure.name}
                      </p>
                      <p className="text-[0.7rem] leading-tight text-red-700">
                        {t('video-preview.importFailed', { defaultValue: 'Not imported: {{message}}', message: failure.message })}
                      </p>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
            <div className="mt-6 flex items-center justify-end gap-3">
              {props.error !== null ? (
                <p role="alert" className="mr-auto text-xs text-red-700">
                  {props.error}
                </p>
              ) : null}
              <Button
                type="button"
                variant="outline"
                className="border-border/80 bg-surface-elevated text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
                onClick={onCancel}
                disabled={isImporting}
              >
                {t('video-preview.cancel', 'Cancel')}
              </Button>
              <Button type="button" onClick={props.onConfirm} disabled={isImporting || props.importCount === 0}>
                {isImporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
                {t('video-preview.importConfirm', { defaultValue: 'Import {{count}} videos', count: props.importCount })}
              </Button>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

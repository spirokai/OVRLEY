import { Activity, Database, FileUp, Route, Sparkles, Trash2 } from 'lucide-react'
import { createPortal } from 'react-dom'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { SectionHeading } from '@/components/ui/section-heading'
import { cn } from '@/lib/utils'
import { useActivityDropZone } from '../hooks/useActivityDropZone'
import { buildActivityDrawerViewModel } from '../utils/activityDrawerUtils'

function FileDragCursor({ position }) {
  if (!position) return null

  return createPortal(
    <div
      aria-hidden="true"
      className="cursor-pointer fixed z-1000 flex h-14 w-14 -translate-x-2 -translate-y-2 items-center justify-center rounded-md border-2 border-card bg-foreground text-card shadow-2xl"
      style={{ left: position.x, top: position.y }}
    >
      <FileUp className="h-10 w-10" strokeWidth={1.5} />
    </div>,
    document.body,
  )
}

function MetricGrid({ metrics, emptyLabel }) {
  if (metrics.length === 0) return <p className="text-[0.7rem] text-muted-foreground/70">{emptyLabel}</p>

  return (
    <div className="grid grid-cols-2 gap-1.5">
      {metrics.map((metric) => (
        <Badge
          key={metric}
          variant="outline"
          className="w-full justify-start rounded-none border-none px-2 py-0.3 text-[0.7rem] font-medium text-foreground/90"
        >
          <span className="truncate">{metric}</span>
        </Badge>
      ))}
    </div>
  )
}

/**
 * Renders the activity import controls and finalized activity details.
 *
 * @param {object} props - Component props.
 * @param {object|null} props.activitySummary - Canonical activity summary normalized by the media store.
 * @param {string|null} props.filename - Imported activity filename, when a standalone activity file is loaded.
 * @param {() => void} props.onBrowseActivity - Opens the activity file picker.
 * @param {(() => void)|null} props.onDeleteActivity - Deletes a standalone imported activity when present.
 * @param {(selections: Array<File|string>) => void} props.onDropActivityFiles - Imports dropped activity files or native paths.
 * @returns {JSX.Element} Rendered drawer content.
 */
export function ActivityDrawerContent({ activitySummary, filename, onBrowseActivity, onDeleteActivity, onDropActivityFiles }) {
  const { dragPosition, dropZoneProps, dropZoneRef, isDraggingFile, isOverDropZone } = useActivityDropZone(onDropActivityFiles)
  const drawerViewModel = activitySummary ? buildActivityDrawerViewModel(activitySummary) : null
  const displayFilename = filename ?? activitySummary?.fileName

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pb-4 thin-scrollbar">
      <FileDragCursor position={isDraggingFile ? dragPosition : null} />

      <Button type="button" className="h-9 w-full gap-2" onClick={onBrowseActivity} aria-keyshortcuts="Alt+A">
        <Activity className="h-4 w-4" />
        Load activity
      </Button>

      <div
        ref={dropZoneRef}
        className={cn(
          'relative mt-4 flex min-h-24 shrink-0 flex-col items-center justify-center rounded-xs border border-dashed border-border/80 bg-surface px-4 text-center transition-colors',
          isOverDropZone && 'border-primary bg-surface-accent-soft/30',
        )}
        {...dropZoneProps}
      >
        <FileUp className={cn('mb-2 h-5 w-5 text-muted-foreground', isOverDropZone && 'text-primary')} />
        <p className="text-xs font-extrabold text-foreground">Drop activity file</p>
        <p className="mt-1 text-[0.75rem] leading-tight text-muted-foreground">GPX, FIT, SRT, IGC, CSV, VBO</p>
      </div>

      {drawerViewModel ? (
        <div className="mt-10 space-y-8 border-t border-border/80 pt-2">
          <section className="space-y-4">
            <div className="flex items-center justify-between pb-2 pl-1 pt-4 text-sm font-extrabold text-foreground">
              <div className="flex min-w-0 items-center gap-2">
                <span className="min-w-0 truncate" title={displayFilename}>
                  {displayFilename}
                </span>
              </div>
              {onDeleteActivity ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 shrink-0 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
                  onClick={onDeleteActivity}
                  aria-label="Delete activity"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              ) : null}
            </div>
            <SectionHeading
              icon={Route}
              title="Details"
              trailing={
                <Badge variant="outline" className="px-2 py-0.2 mr-1 text-[0.75rem] font-extrabold rounded-none text-primary border-primary">
                  {drawerViewModel.formatLabel}
                </Badge>
              }
              variant="drawer"
            />
            <dl className="grid grid-cols-2 gap-x-4.5 gap-y-2 px-2 text-xs">
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

          <section className="space-y-4">
            <SectionHeading icon={Database} title="Extracted" variant="drawer" />
            <MetricGrid metrics={drawerViewModel.metricGroups.extracted} emptyLabel="No data were extracted" />
          </section>

          <section className="space-y-4">
            <SectionHeading icon={Sparkles} title="Calculated" variant="drawer" />
            <MetricGrid metrics={drawerViewModel.metricGroups.derived} emptyLabel="No data were calculated" />
          </section>
        </div>
      ) : null}
    </div>
  )
}

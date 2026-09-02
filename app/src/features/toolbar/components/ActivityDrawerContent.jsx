import { Activity, Database, Route, Sparkles, Trash2 } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { SectionHeading } from '@/components/ui/section-heading'
import { useFileDropZone } from '../hooks/useFileDropZone'
import { buildActivityDrawerViewModel } from '../utils/activityDrawerUtils'
import { FileDragCursor, FileDropZone } from './FileDropZone'
import { useTranslation } from 'react-i18next'

function MetricGrid({ metrics, emptyLabel }) {
  if (metrics.length === 0) return <p className="text-[0.7rem] text-muted-foreground/90 px-2">{emptyLabel}</p>

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
  const { t } = useTranslation()
  const { dragPosition, dropZoneProps, dropZoneRef, isDraggingFile, isOverDropZone } = useFileDropZone(onDropActivityFiles)
  const drawerViewModel = activitySummary ? buildActivityDrawerViewModel(activitySummary, t) : null
  const displayFilename = filename ?? activitySummary?.fileName

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pb-4 thin-scrollbar">
      <FileDragCursor position={isDraggingFile ? dragPosition : null} />

      <Button type="button" className="h-9 w-full gap-2" onClick={onBrowseActivity} aria-keyshortcuts="Alt+A">
        <Activity className="h-4 w-4" />
        {t('toolbar.loadActivity', 'Load activity')}
      </Button>

      <FileDropZone
        dropZoneRef={dropZoneRef}
        dropZoneProps={dropZoneProps}
        isOverDropZone={isOverDropZone}
        label={t('toolbar.dropActivityFile', 'Drop activity file')}
        sublabel="GPX, FIT, SRT, IGC, CSV, VBO"
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
              {onDeleteActivity ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 shrink-0 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
                  onClick={onDeleteActivity}
                  aria-label={t('toolbar.deleteActivity', 'Delete activity')}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              ) : null}
            </div>
            <SectionHeading
              icon={Route}
              title={t('toolbar.details', 'Details')}
              trailing={
                <Badge variant="outline" className="px-2 py-0.2 mr-1 text-[0.75rem] font-extrabold rounded-none text-primary border-primary">
                  {drawerViewModel.formatLabel}
                </Badge>
              }
              variant="drawer"
            />
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

          <section className="space-y-4">
            <SectionHeading icon={Database} title={t('toolbar.extracted', 'Extracted')} variant="drawer" />
            <MetricGrid metrics={drawerViewModel.metricGroups.extracted} emptyLabel={t('toolbar.noDataExtracted', 'No data were extracted')} />
          </section>

          <section className="space-y-4">
            <SectionHeading icon={Sparkles} title={t('toolbar.calculated', 'Calculated')} variant="drawer" />
            <MetricGrid metrics={drawerViewModel.metricGroups.derived} emptyLabel={t('toolbar.noDataCalculated', 'No data were calculated')} />
          </section>
        </div>
      ) : null}
    </div>
  )
}

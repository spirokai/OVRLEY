import { Activity, FilePlus2, Film, FolderOpen, Save } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { SectionHeading } from '@/components/ui/section-heading'
import { useTranslation } from 'react-i18next'

function ProjectSource({ icon: Icon, label, filename, path }) {
  if (!path) return null

  return (
    <section className="min-w-0 space-y-3 px-1">
      <SectionHeading
        icon={Icon}
        title={label}
        trailing={
          <p className="max-w-32 truncate text-xs font-medium text-foreground/90" title={filename}>
            {filename}
          </p>
        }
        variant="drawer"
      />
      <p className="min-w-0 break-all text-xs text-muted-foreground" title={path}>
        {path}
      </p>
    </section>
  )
}

export function ProjectsDrawerContent({
  projectName,
  projectPath,
  activityFilename,
  activityPath,
  videoFilename,
  videoPath,
  status,
  busy,
  onNew,
  onOpen,
  onSave,
  onSaveAs,
}) {
  const { t } = useTranslation()
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto px-3 pb-4 thin-scrollbar">
      <div className="space-y-3">
        <div className="grid grid-cols-2 gap-2">
          <Button type="button" className="w-full gap-2" disabled={busy} onClick={onNew} aria-keyshortcuts="Mod+N">
            <FilePlus2 className="size-4" /> {t('toolbar.newProject', 'New Project')}
          </Button>
          <Button type="button" variant="secondary" className="w-full gap-2" disabled={busy} onClick={onOpen} aria-keyshortcuts="Mod+O">
            <FolderOpen className="size-4" /> {t('toolbar.openProject', 'Open Project')}
          </Button>
        </div>
        <Button
          type="button"
          variant="secondary"
          className="w-full gap-2"
          disabled={busy || status === 'Saved'}
          onClick={onSave}
          aria-keyshortcuts="Mod+S"
        >
          <Save className="size-4" /> {t('toolbar.saveProject', 'Save Project')}
        </Button>
        <Button type="button" variant="secondary" className="w-full" disabled={busy} onClick={onSaveAs} aria-keyshortcuts="Mod+Shift+S">
          <Save className="size-4" /> {t('toolbar.saveProjectAs', 'Save Project As')}
        </Button>
      </div>

      {projectPath ? (
        <div className="mt-10 min-w-0 space-y-6 border-t border-border/80 pt-2">
          <div className="min-w-0 pb-2 pl-1 pt-4 text-sm font-extrabold text-foreground">
            <span className="block min-w-0 truncate" title={projectName}>
              {projectName}
            </span>
          </div>
          <div className="min-w-0 space-y-8">
            <ProjectSource icon={Activity} label={t('toolbar.activity', 'Activity')} filename={activityFilename} path={activityPath} />
            <ProjectSource icon={Film} label={t('toolbar.video', 'Video')} filename={videoFilename} path={videoPath} />
          </div>
        </div>
      ) : null}
    </div>
  )
}

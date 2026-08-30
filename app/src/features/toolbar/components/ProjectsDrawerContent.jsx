import { FilePlus2, FolderOpen, Save } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'

export function ProjectsDrawerContent({ projectName, projectPath, status, busy, onNew, onOpen, onSave, onSaveAs }) {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-3 pb-4 thin-scrollbar">
      <Button type="button" className="w-full gap-2" disabled={busy} onClick={onNew} aria-keyshortcuts="Mod+N">
        <FilePlus2 className="size-4" /> New Project
      </Button>
      <Button type="button" variant="secondary" className="w-full gap-2" disabled={busy} onClick={onOpen} aria-keyshortcuts="Mod+O">
        <FolderOpen className="size-4" /> Open Project
      </Button>
      <div className="grid grid-cols-2 gap-2">
        <Button
          type="button"
          variant="secondary"
          className="w-full gap-2"
          disabled={busy || status === 'Saved'}
          onClick={onSave}
          aria-keyshortcuts="Mod+S"
        >
          <Save className="size-4" /> Save Project
        </Button>
        <Button type="button" variant="secondary" className="w-full" disabled={busy} onClick={onSaveAs} aria-keyshortcuts="Mod+Shift+S">
          Save Project As
        </Button>
      </div>
      <div className="rounded-md border border-border bg-background/40 p-3">
        <div className="flex items-center justify-between gap-3">
          <span className="min-w-0 truncate text-sm font-bold" title={projectPath ?? undefined}>
            {projectName || 'Unsaved project'}
          </span>
          <Badge variant="outline">{status}</Badge>
        </div>
        {projectPath ? (
          <p className="mt-1 truncate text-xs text-muted-foreground" title={projectPath}>
            {projectPath}
          </p>
        ) : null}
      </div>
    </div>
  )
}

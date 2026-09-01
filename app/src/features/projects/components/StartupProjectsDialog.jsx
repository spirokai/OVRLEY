import { FileBox, Plus } from 'lucide-react'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { cn } from '@/lib/utils'

function ProjectCard({ kind, name, title, disabled, onClick }) {
  const isNewProject = kind === 'new'

  return (
    <button type="button" className="group min-w-0 cursor-pointer text-center" disabled={disabled} onClick={onClick} title={title}>
      <span
        className={cn(
          'flex aspect-video w-full items-center justify-center rounded-xs border border-border/80 bg-surface text-muted-foreground transition-colors group-hover:border-primary group-hover:bg-surface-elevated group-hover:text-primary group-focus-visible:border-primary group-focus-visible:outline-none group-disabled:opacity-50',
          isNewProject && 'border-dashed',
        )}
      >
        {isNewProject ? (
          <span className="flex size-18 items-center justify-center rounded-full bg-foreground/10 text-foreground/70 transition-colors group-hover:bg-foreground/15 group-hover:text-foreground">
            <Plus className="size-12" strokeWidth={3} aria-hidden="true" />
          </span>
        ) : (
          <FileBox className="size-10" strokeWidth={1.5} aria-hidden="true" />
        )}
      </span>
      <span className="mt-2 block truncate text-xs font-medium text-foreground">{name}</span>
    </button>
  )
}

export default function StartupProjectsDialog({ open, projects, openingPath, onDismiss, onNewProject, onOpenProject }) {
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onDismiss()}>
      <DialogContent
        className="flex max-h-[80vh] w-full max-w-4xl flex-col rounded-sm border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/70 px-4 backdrop-blur-md"
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <DialogTitle className="text-sm font-semibold text-primary">Open a project</DialogTitle>
        <DialogDescription className="mt-2 text-xs leading-5 text-muted-foreground">
          Start a new project or continue with an existing one.
        </DialogDescription>

        <div className="mt-6 grid min-h-120 max-h-200 content-start items-start grid-cols-[repeat(auto-fill,minmax(12.7575rem,1fr))] gap-x-4 gap-y-6 overflow-y-auto pr-1 thin-scrollbar">
          <ProjectCard kind="new" name="New Project" disabled={openingPath !== null} onClick={onNewProject} />

          {projects.map((project) => (
            <ProjectCard
              key={project.path}
              kind="existing"
              name={project.name}
              title={project.path}
              disabled={openingPath !== null}
              onClick={() => onOpenProject(project.path)}
            />
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}

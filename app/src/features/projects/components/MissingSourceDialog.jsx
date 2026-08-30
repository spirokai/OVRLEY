import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'

export function MissingSourceDialog({ sourceRole, open, onLocate, onLoadAnyway, onCancel }) {
  return (
    <Dialog open={open}>
      <DialogContent
        className="w-full max-w-lg rounded-sm border border-accent-border/80 bg-card/95 p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/92 px-4 backdrop-blur-md"
      >
        <DialogTitle className="text-lg font-bold">{sourceRole === 'video' ? 'Video' : 'Activity'} source not found</DialogTitle>
        <DialogDescription className="mt-2 text-sm text-muted-foreground">
          {`The ${sourceRole} file does not seem to exist. Locate the moved file or cancel without changing the current session.`}
        </DialogDescription>
        <div className="mt-6 flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="button" variant="secondary" onClick={onLoadAnyway}>
            Load Anyway
          </Button>
          <Button type="button" onClick={onLocate}>
            Locate
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

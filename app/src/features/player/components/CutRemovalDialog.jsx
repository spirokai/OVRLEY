import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'

/**
 * Asks what happens to the merged part when a removed cut separates a part that is
 * included in the export from one that is excluded.
 * @param {object} props Props from `useCutRemovalDialog`.
 * @param {boolean} props.open Whether a cut removal awaits the choice.
 * @param {function(): void} props.onKeepCut Keeps the cut.
 * @param {function(boolean): void} props.onMerge Removes the cut; takes whether the merged part is excluded.
 */
export function CutRemovalDialog({ open, onKeepCut, onMerge }) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onKeepCut()}>
      <DialogContent
        className="w-full max-w-lg rounded-sm border border-accent-border/80 bg-card p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-sm"
      >
        <DialogTitle className="text-lg font-bold">{t('player.cutRemovalTitle', 'Remove this cut?')}</DialogTitle>
        <DialogDescription className="mt-2 text-sm text-muted-foreground">
          {t(
            'player.cutRemovalDescription',
            'One side of this cut is included in the export and the other is excluded. Choose what happens to the merged part.',
          )}
        </DialogDescription>
        <div className="mt-6 flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onKeepCut}>
            {t('player.cutRemovalCancel', 'Keep the cut')}
          </Button>
          <Button type="button" variant="secondary" onClick={() => onMerge(true)}>
            {t('player.cutRemovalExcludeBoth', 'Exclude both')}
          </Button>
          <Button type="button" onClick={() => onMerge(false)}>
            {t('player.cutRemovalIncludeBoth', 'Include both')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

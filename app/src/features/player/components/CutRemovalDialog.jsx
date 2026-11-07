import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import useStore from '@/store/useStore'

/**
 * Asks what happens to the merged part when a removed cut separates a part that is
 * included in the export from one that is excluded.
 */
export function CutRemovalDialog() {
  const { t } = useTranslation()
  const { isOpen, resolveCutRemoval, dismissCutRemoval } = useStore(
    useShallow((state) => ({
      isOpen: state.cutRemovalRequest !== null,
      resolveCutRemoval: state.resolveCutRemoval,
      dismissCutRemoval: state.dismissCutRemoval,
    })),
  )
  return (
    <Dialog open={isOpen} onOpenChange={(nextOpen) => !nextOpen && dismissCutRemoval()}>
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
          <Button type="button" variant="ghost" onClick={dismissCutRemoval}>
            {t('player.cutRemovalCancel', 'Keep the cut')}
          </Button>
          <Button type="button" variant="secondary" onClick={() => resolveCutRemoval(true)}>
            {t('player.cutRemovalExcludeBoth', 'Exclude both')}
          </Button>
          <Button type="button" onClick={() => resolveCutRemoval(false)}>
            {t('player.cutRemovalIncludeBoth', 'Include both')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

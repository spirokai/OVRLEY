import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useTranslation } from 'react-i18next'

/**
 * Asks once per project for the default transition between videos and the gap
 * below which consecutive videos count as one continuous recording.
 */
export function TransitionDefaultsDialog({
  open,
  typeOptions,
  type,
  duration,
  threshold,
  durationDisabled,
  error,
  onTypeChange,
  onDurationChange,
  onThresholdChange,
  onConfirm,
  onDismiss,
}) {
  const { t } = useTranslation()
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onDismiss()}>
      <DialogContent
        className="w-full max-w-lg rounded-sm border border-accent-border/80 bg-card p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
      >
        <DialogTitle className="text-lg font-bold">{t('video-preview.transitionDefaultsTitle', 'Transitions between videos')}</DialogTitle>
        <DialogDescription className="mt-2 text-sm text-muted-foreground">
          {t(
            'video-preview.transitionDefaultsDescription',
            'Choose how videos are joined. Videos closer together than the continuity threshold are treated as one recording and joined without a transition. You can change every join later.',
          )}
        </DialogDescription>
        <form
          className="mt-4 grid grid-cols-[minmax(0,1fr)_10rem] items-center gap-x-4 gap-y-3"
          onSubmit={(event) => {
            event.preventDefault()
            onConfirm()
          }}
        >
          <Label htmlFor="transition-default-type">{t('video-preview.transitionType', 'Transition')}</Label>
          <Select value={type} onValueChange={onTypeChange}>
            <SelectTrigger id="transition-default-type">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {typeOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Label htmlFor="transition-default-duration">{t('video-preview.transitionDuration', 'Duration (seconds)')}</Label>
          <Input
            id="transition-default-duration"
            inputMode="decimal"
            value={duration}
            disabled={durationDisabled}
            onChange={(event) => onDurationChange(event.target.value)}
          />
          <Label htmlFor="transition-default-threshold">{t('video-preview.continuityThreshold', 'Continuity threshold (seconds)')}</Label>
          <Input
            id="transition-default-threshold"
            inputMode="decimal"
            value={threshold}
            onChange={(event) => onThresholdChange(event.target.value)}
          />
          {error && (
            <p role="alert" className="col-span-2 text-xs text-destructive">
              {error}
            </p>
          )}
          <div className="col-span-2 mt-3 flex justify-end">
            <Button type="submit">{t('video-preview.ok', 'OK')}</Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

import { useMemo } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'
import { useTranslation } from 'react-i18next'
import { buildVideoImportReportSections } from '../utils/videoImportReport'

/**
 * Lists the files of an import that need the user's attention.
 */
export function VideoImportReportDialog({ open, report, onClose }) {
  const { t } = useTranslation()
  const sections = useMemo(() => (report ? buildVideoImportReportSections(report, t) : []), [report, t])
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onClose()}>
      <DialogContent
        className="w-full max-w-lg rounded-sm border border-accent-border/80 bg-card p-6 shadow-2xl shadow-background/50"
        overlayClassName="absolute inset-0 z-120 flex items-center justify-center bg-surface-overlay/82 px-4 backdrop-blur-md"
      >
        <DialogTitle className="text-lg font-bold">{t('video-preview.importReportTitle', 'Video import')}</DialogTitle>
        <DialogDescription className="sr-only">{t('video-preview.importReportDescription', 'Files that need your attention')}</DialogDescription>
        <div className="mt-2 max-h-[60vh] space-y-5 overflow-y-auto thin-scrollbar">
          {report?.notice && <p className="text-sm text-foreground">{report.notice}</p>}
          {sections.map((section) => (
            <section key={section.id} aria-label={section.title}>
              <h3 className="text-sm font-bold text-foreground">{section.title}</h3>
              {section.description && <p className="mt-1 text-xs text-muted-foreground">{section.description}</p>}
              <ul className="mt-2 space-y-1">
                {section.items.map((item) => (
                  <li key={item.key} className="text-xs">
                    <span className="font-medium break-all text-foreground">{item.name}</span>
                    {item.detail && <span className="block break-words text-muted-foreground">{item.detail}</span>}
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
        <div className="mt-6 flex justify-end">
          <Button type="button" onClick={onClose}>
            {t('video-preview.ok', 'OK')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

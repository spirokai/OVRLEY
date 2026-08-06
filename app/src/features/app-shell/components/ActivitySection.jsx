/**
 * Left column of the app header — activity file and video import controls.
 * Pure presentational component.
 */

import { Button } from '@/components/ui/button'
import { Activity, Film, X } from 'lucide-react'

/**
 * Renders the activity and video controls in the app header.
 *
 * @param {object} props
 * @param {string} props.activityLabel - Label for the activity file button.
 * @param {function} props.onOpenActivityFile - Opens the activity file picker.
 * @param {boolean} props.debugModeEnabled - Whether debug-only media features are enabled.
 * @param {string|null} props.appVersion - Build-time app version display label.
 * @param {string|null} props.importedMediaFilename - Filename of the imported background media, or null.
 * @param {function} props.handleImportVideo - Opens the video import picker.
 * @param {function} props.clearImportedVideo - Clears the imported video.
 * @returns {JSX.Element} Rendered component.
 */
export default function ActivitySection({
  activityLabel,
  onOpenActivityFile,
  debugModeEnabled,
  appVersion,
  importedMediaFilename,
  handleImportVideo,
  clearImportedVideo,
}) {
  return (
    <div className="flex min-w-0 items-center gap-6 overflow-hidden">
      <div className="flex shrink-0 items-center gap-3">
        <img src="/logo.svg" alt="OVRLEY" className="h-5 pr-3" />
        <div className="h-8 w-px shrink-0 bg-border/60" />
        {appVersion ? (
          <div className="text-[0.8rem] font-semibold tabular-nums text-muted-foreground/70 normal-case leading-none pt-1 pl-3">{appVersion}</div>
        ) : null}
      </div>

      <div className="h-8 w-px shrink-0 bg-border/60" />

      <div className="min-w-0 flex-1 overflow-hidden">
        <div className="flex min-w-0 items-center gap-2">
          <Button className="mr-2 h-9 w-48 shrink-0 gap-2 border-border/70 px-5" onClick={onOpenActivityFile} aria-keyshortcuts="Alt+A">
            <Activity className="h-3.5 w-3.5" />
            <span className="max-w-28 truncate">{activityLabel}</span>
          </Button>

          {importedMediaFilename ? (
            <div className="w-48 mr-2 flex h-9 shrink-0 items-center rounded-sm border border-border/70 bg-surface-elevated pl-3 pr-2 text-xs text-foreground justify-between">
              <div className="flex items-center gap-2 truncate">
                <Film className="mr-2 h-4 w-4 text-primary" />
                <span className="truncate">{importedMediaFilename}</span>
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="ml-1 h-6 w-6 text-muted-foreground hover:bg-accent/15 hover:text-foreground"
                onClick={clearImportedVideo}
              >
                <X className="h-3.5 w-3.5" />
              </Button>
            </div>
          ) : (
            <Button
              variant="outline"
              className="w-48 mr-2 h-9 shrink-0 gap-2 border-border/70 px-5 text-muted-foreground hover:text-foreground text-sm"
              onClick={handleImportVideo}
              aria-keyshortcuts="Mod+I"
            >
              <Film className="h-3.5 w-3.5" />
              <span className="truncate">{debugModeEnabled ? 'Import Video / Image' : 'Import Video'}</span>
            </Button>
          )}
        </div>
      </div>
    </div>
  )
}

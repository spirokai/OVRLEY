/**
 * Renders imported video information, sync offset controls, and resolution warnings.
 * Pure presentational — all data comes from props.
 *
 * @param {object} props
 * @param {string} props.importedVideoPath - Imported video file path.
 * @param {number} props.importedVideoDuration - Video duration in seconds.
 * @param {number} props.importedVideoFps - Video frame rate.
 * @param {object} props.importedVideoResolution - Video resolution ({ width, height }).
 * @param {number} props.importedVideoCreationTime - Video creation timestamp.
 * @param {string|null} props.videoSyncWarning - Sync warning message (or null).
 * @param {boolean} props.videoResolutionMismatch - Whether overlay/video resolutions differ.
 * @param {string} props.offsetInput - Current sync offset input value.
 * @param {function} props.onOffsetInputChange - Callback to update offset input.
 * @param {function} props.onOffsetBlur - Callback on offset blur.
 * @param {function} props.onIncrement - Callback for increment/decrement.
 * @param {object} props.activitySummary - Activity summary data (or null).
 * @param {function} props.onComputeVideoSync - Callback to auto-sync.
 * @returns {JSX.Element} Rendered video sync section.
 */

import { Label } from '@/components/ui/label'
import { BlurInput } from '@/components/ui/blur-input'
import { Separator } from '@/components/ui/separator'
import { Video, AlertTriangle, ChevronUp, ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui/button'

export default function VideoSyncSection({
  importedVideoDuration,
  importedVideoFps,
  importedVideoResolution,
  importedVideoCreationTime,
  videoSyncWarning,
  videoResolutionMismatch,
  offsetInput,
  onOffsetInputChange,
  onOffsetBlur,
  onIncrement,
  activitySummary,
  onComputeVideoSync,
}) {
  return (
    <div className="space-y-4 pt-4">
      <div className="flex items-center gap-2 mb-2">
        <Video className="h-4 w-4 text-primary" />
        <h4 className="text-xs font-bold text-muted-foreground uppercase tracking-widest">Video</h4>
        <Separator className="flex-1" />
      </div>

      <div className="space-y-2 text-xs text-muted-foreground px-1 pt-2">
        <div className="flex justify-between">
          <b>Data:</b>
          <span className="text-xs font-normal text-foreground/70">
            {importedVideoDuration
              ? `${Math.floor(importedVideoDuration / 60)}:${Math.floor(importedVideoDuration % 60)
                  .toString()
                  .padStart(2, '0')} min`
              : 'Unknown'}{' '}
            · {importedVideoFps ? `${Math.round(importedVideoFps * 100) / 100} fps` : 'Unknown'} ·{' '}
            {importedVideoResolution ? `${importedVideoResolution.width}×${importedVideoResolution.height}` : 'Unknown'}
          </span>
        </div>
        <div className="flex justify-between">
          <b>Created at:</b>
          <span className="text-xs font-normal text-foreground/70">
            {importedVideoCreationTime ? new Date(importedVideoCreationTime).toLocaleString() : 'Unknown'}
          </span>
        </div>
      </div>

      {videoSyncWarning && (
        <div className="flex gap-2 items-center rounded-md bg-destructive/25 p-2 pl-4 text-destructive">
          <AlertTriangle className="h-3 w-3 shrink-0" />
          <p className="text-[0.65rem] font-semibold leading-tight">{videoSyncWarning}</p>
        </div>
      )}

      {videoResolutionMismatch && (
        <div className="flex gap-2 items-center rounded-md bg-destructive/25 p-2 pl-4 text-destructive">
          <AlertTriangle className="h-3 w-3 shrink-0" />
          <p className="text-[0.65rem] font-semibold leading-tight">Overlay and video resolutions do not match</p>
        </div>
      )}

      {activitySummary?.startTime && (
        <div className="space-y-1">
          <Label className="text-[10px] text-muted-foreground uppercase font-bold pb-2!">Sync Offset</Label>
          <div className="grid grid-cols-2 gap-4">
            <div className="relative flex-1">
              <BlurInput
                type="text"
                value={offsetInput}
                onChange={(e) => onOffsetInputChange(e.target.value)}
                onBlur={(e) => onOffsetBlur(e.target.value)}
                className="h-9 text-xs pr-11 w-full border border-border/70"
                placeholder="Seconds or MM:SS"
              />
              <div className="absolute inset-y-1 right-1 flex w-5 flex-col overflow-hidden rounded border border-none bg-surface-strong">
                <button
                  type="button"
                  className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => onIncrement(0.1)}
                >
                  <ChevronUp className="h-3 w-3" />
                </button>
                <div className="h-px bg-border/60" />
                <button
                  type="button"
                  className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50 cursor-pointer"
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => onIncrement(-0.1)}
                >
                  <ChevronDown className="h-3 w-3" />
                </button>
              </div>
            </div>
            <Button
              variant="outline"
              size="sm"
              className="h-9 border-border/80 bg-surface-elevated px-3 text-xs font-semibold text-foreground shadow-xs hover:bg-surface-strong hover:text-foreground"
              disabled={!activitySummary}
              onClick={onComputeVideoSync}
            >
              Auto-sync
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

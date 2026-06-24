/**
 * Displays render progress bar, status message, ETA, and cancel button.
 * Pure presentational component — all data comes from props, no store access.
 *
 * @param {object} props
 * @param {object} props.renderProgress - Current render progress state from the store.
 * @param {number} props.renderProgress.percent - Completion percentage (0-100).
 * @param {number} props.renderProgress.current - Current frame number.
 * @param {number} props.renderProgress.total - Total frame count.
 * @param {string} props.renderProgress.message - Status message from the backend.
 * @param {number|null} props.renderProgress.estimatedSecondsRemaining - Estimated remaining time.
 * @param {number|null} props.renderProgress.renderingFps - Estimated output-frame-equivalent production FPS.
 * @param {number} props.renderProgress.encoded - Number of encoded frames.
 * @param {string[]} [props.renderSummaryItems] - Compact render settings summary fragments.
 * @param {function} props.onCancel - Async callback invoked when user clicks cancel.
 */

import { useEffect, useState } from 'react'
import { Activity, Film, Loader2, Timer } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { formatFps, formatTime } from '../utils/codecUtils'

function RenderProgressPanel({ renderProgress, renderSummaryItems = [], onCancel }) {
  const [isCancelling, setIsCancelling] = useState(false)

  const { percent, current, total, message, estimatedSecondsRemaining, renderingFps, encoded } = renderProgress

  useEffect(() => {
    if (renderProgress.status !== 'rendering') {
      setIsCancelling(false)
    }
  }, [renderProgress.status])

  const handleCancel = async () => {
    try {
      setIsCancelling(true)
      await onCancel()
    } catch (error) {
      console.error('Failed to cancel render:', error)
      setIsCancelling(false)
    }
  }

  const isFinalizing = percent >= 100

  let subMessage = message || 'Processing frames...'
  if (isFinalizing) {
    subMessage = encoded && total > 0 ? `Encoding: ${encoded.toLocaleString()} / ${total.toLocaleString()} frames` : 'Encoding output file...'
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col items-center gap-4 text-center">
        <div className="relative flex h-16 w-16 items-center justify-center rounded-full bg-surface-accent-soft">
          <Loader2 className="absolute h-10 w-10 animate-spin text-primary" />
          <Film className="h-5 w-5 text-primary/60" />
        </div>
        <div>
          <h2 className="text-xl font-bold text-foreground">{isFinalizing ? 'Finalizing Video' : 'Exporting Overlay'}</h2>
          <p className="text-sm text-muted-foreground">{subMessage}</p>
          {renderSummaryItems.length > 0 && (
            <p className="pt-8 flex flex-wrap items-center justify-center gap-x-1 gap-y-1 text-[0.65rem] text-muted-foreground/55">
              {renderSummaryItems.map((item, index) => (
                <span key={`${item}-${index}`} className="inline-flex items-center">
                  {index > 0 && <span className="mr-1 text-muted-foreground/25">/</span>}
                  <span>{item}</span>
                </span>
              ))}
            </p>
          )}
        </div>
      </div>

      <div className="space-y-3 pt-6">
        <div className="flex justify-between text-xs font-medium">
          <span className="text-primary">{percent}% Complete</span>
          <span className="text-muted-foreground">
            {current.toLocaleString()} / {total.toLocaleString()} frames
          </span>
        </div>
        <Progress value={percent} className="h-2 bg-surface-strong" />
      </div>

      {!isFinalizing && (
        <div className="flex items-center justify-center gap-6 pt-2">
          <div className="flex flex-col items-center">
            <div className="mb-1 flex items-center gap-1.5 text-muted-foreground">
              <Activity className="h-3.5 w-3.5" />
              <span className="text-[10px] font-bold uppercase tracking-wider">Render FPS</span>
            </div>
            <span className="text-lg font-mono font-bold text-foreground">{formatFps(renderingFps)}</span>
          </div>
          <div className="flex flex-col items-center">
            <div className="mb-1 flex items-center gap-1.5 text-muted-foreground">
              <Timer className="h-3.5 w-3.5" />
              <span className="text-[10px] font-bold uppercase tracking-wider">Est. Remaining</span>
            </div>
            <span className="text-lg font-mono font-bold text-foreground">{formatTime(estimatedSecondsRemaining)}</span>
          </div>
        </div>
      )}

      <div className="flex justify-center pt-2">
        <Button
          type="button"
          variant="ghost"
          className="text-muted-foreground hover:bg-surface-accent-soft hover:text-highlight"
          onClick={handleCancel}
          disabled={isCancelling}
        >
          {isCancelling ? (
            <>
              <Loader2 className="h-3 w-3 animate-spin" />
              Cancelling...
            </>
          ) : (
            'Cancel'
          )}
        </Button>
      </div>

      <p className="text-center text-[10px] italic text-muted-foreground/50">Please keep the application open during rendering</p>
    </div>
  )
}

export default RenderProgressPanel

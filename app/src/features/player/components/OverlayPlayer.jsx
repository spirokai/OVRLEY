/**
 * Renders the overlay player portion of the application interface.
 */

import { Pause, Play, RotateCcw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Slider } from '@/components/ui/slider'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import useOverlayPlayerState from '../hooks/useOverlayPlayerState'
import { formatTimelineTime } from '../utils/playerTimeline'

/**
 * Renders the overlay player component.
 *
 * @param {object} props - Component props.
 * @param {string} props.backgroundMode - Selected canvas background style.
 * @returns {JSX.Element} Rendered component output.
 */
export default function OverlayPlayer({ backgroundMode }) {
  const {
    clampedPlayhead,
    displayedPlayhead,
    handlePause,
    handlePlay,
    handleReset,
    handleTimelineChange,
    handleTimelineCommit,
    hasActivity,
    importedVideoDuration,
    importedVideoPath,
    isPlaying,
    totalDuration,
    videoSyncOffsetSeconds,
  } = useOverlayPlayerState({ backgroundMode })

  return (
    <div className={hasActivity ? 'shrink-0 border-border/70 bg-black/30 px-5 py-2 backdrop-blur-sm' : 'hidden'}>
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-1.5 rounded-2xl border border-border/70 p-1 shadow-sm">
          <SimpleTooltip side="top" content="Play live preview">
            <Button
              type="button"
              size="icon-sm"
              variant={isPlaying ? 'secondary' : 'default'}
              className="rounded-xl"
              disabled={!hasActivity || isPlaying}
              onClick={handlePlay}
            >
              <Play className="h-4 w-4" />
            </Button>
          </SimpleTooltip>
          <SimpleTooltip side="top" content="Pause playback">
            <Button type="button" size="icon-sm" variant="ghost" className="rounded-xl" disabled={!hasActivity || !isPlaying} onClick={handlePause}>
              <Pause className="h-4 w-4" />
            </Button>
          </SimpleTooltip>
          <SimpleTooltip side="top" content="Reset to start">
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              className="rounded-xl"
              disabled={!hasActivity || clampedPlayhead <= 0}
              onClick={handleReset}
            >
              <RotateCcw className="h-4 w-4" />
            </Button>
          </SimpleTooltip>
        </div>

        <div className="flex min-w-0 flex-1 items-center gap-3">
          {/* <span className="shrink-0 text-xs font-medium tabular-nums text-muted-foreground"></span> */}
          <div className="relative min-w-0 flex-1">
            <Slider
              min={0}
              max={Math.max(totalDuration, 1)}
              step={0.1}
              value={[displayedPlayhead]}
              disabled={!hasActivity}
              onValueChange={handleTimelineChange}
              onValueCommit={handleTimelineCommit}
              trackChildren={
                importedVideoPath &&
                totalDuration > 0 && (
                  <div
                    className="absolute inset-y-0 bg-accent"
                    style={{
                      left: `${Math.max(0, (videoSyncOffsetSeconds / totalDuration) * 100)}%`,
                      width: `${Math.min(100, (importedVideoDuration / totalDuration) * 100)}%`,
                    }}
                  />
                )
              }
            />
          </div>
          <span className="shrink-0 text-xs font-medium tabular-nums text-muted-foreground w-30 justify-end flex pr-2">
            {formatTimelineTime(displayedPlayhead)} / {formatTimelineTime(totalDuration)}
          </span>
        </div>
      </div>
    </div>
  )
}

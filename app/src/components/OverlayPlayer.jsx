import { useEffect, useMemo, useRef, useState } from 'react'
import { Pause, Play, RotateCcw } from 'lucide-react'
import useStore from '@/store/useStore'
import { Button } from '@/components/ui/button'
import { Slider } from '@/components/ui/slider'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { getEffectivePreviewFps } from './overlay-editor/previewInterpolation'

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

function formatTimelineTime(value) {
  const safeValue = Math.max(0, Math.floor(Number(value) || 0))
  const hours = Math.floor(safeValue / 3600)
  const minutes = Math.floor((safeValue % 3600) / 60)
  const seconds = safeValue % 60

  if (hours > 0) {
    return [hours, minutes, seconds]
      .map((part, index) => String(part).padStart(index === 0 ? 1 : 2, '0'))
      .join(':')
  }

  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`
}

export default function OverlayPlayer() {
  const activitySummary = useStore((state) => state.activitySummary)
  const sceneFps = useStore((state) => state.config?.scene?.fps ?? 30)
  const dummyDurationSeconds = useStore((state) => state.dummyDurationSeconds)
  const endSecond = useStore((state) => state.endSecond)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const updateRate = useStore((state) => state.updateRate)
  const setSelectedSecond = useStore((state) => state.setSelectedSecond)
  const setSelectedSecondTransient = useStore(
    (state) => state.setSelectedSecondTransient,
  )
  const [isPlaying, setIsPlaying] = useState(false)
  const playbackAnchorRef = useRef({
    startedAtMs: 0,
    startedSecond: 0,
  })
  const totalDurationRef = useRef(0)
  const previewFrameRef = useRef(-1)

  const totalDuration = useMemo(() => {
    const metadataDuration = Number(activitySummary?.durationSeconds) || 0
    const fallbackDuration = Number(dummyDurationSeconds) || 0

    return Math.max(
      metadataDuration,
      fallbackDuration,
      Number(endSecond) || 0,
      0,
    )
  }, [activitySummary?.durationSeconds, dummyDurationSeconds, endSecond])

  const hasActivity = Boolean(activitySummary && totalDuration > 0)
  const clampedPlayhead = clamp(Number(selectedSecond) || 0, 0, totalDuration)
  const effectivePreviewFps = useMemo(
    () => getEffectivePreviewFps(sceneFps, updateRate),
    [sceneFps, updateRate],
  )

  useEffect(() => {
    totalDurationRef.current = totalDuration
  }, [totalDuration])

  useEffect(() => {
    if (!hasActivity) {
      setIsPlaying(false)
      playbackAnchorRef.current = {
        startedAtMs: 0,
        startedSecond: 0,
      }
      return
    }

    if (clampedPlayhead !== selectedSecond) {
      setSelectedSecondTransient(clampedPlayhead)
    }
  }, [clampedPlayhead, hasActivity, selectedSecond, setSelectedSecondTransient])

  useEffect(() => {
    if (!isPlaying || !hasActivity) return undefined

    let animationFrameId = 0

    const tick = (now) => {
      const elapsedSeconds =
        (now - playbackAnchorRef.current.startedAtMs) / 1000
      const nextSecond =
        playbackAnchorRef.current.startedSecond + elapsedSeconds
      const safeDuration = totalDurationRef.current

      if (nextSecond >= safeDuration) {
        setSelectedSecondTransient(safeDuration)
        setIsPlaying(false)
        playbackAnchorRef.current = {
          startedAtMs: 0,
          startedSecond: safeDuration,
        }
        previewFrameRef.current = -1
        return
      }

      const frameIndex = Math.floor(nextSecond * effectivePreviewFps)

      if (frameIndex !== previewFrameRef.current) {
        previewFrameRef.current = frameIndex
        setSelectedSecondTransient(
          clamp(frameIndex / effectivePreviewFps, 0, safeDuration),
        )
      }

      animationFrameId = window.requestAnimationFrame(tick)
    }

    animationFrameId = window.requestAnimationFrame(tick)

    return () => window.cancelAnimationFrame(animationFrameId)
  }, [effectivePreviewFps, hasActivity, isPlaying, setSelectedSecondTransient])

  const handlePlay = () => {
    if (!hasActivity) return

    const initialSecond = clampedPlayhead >= totalDuration ? 0 : clampedPlayhead
    playbackAnchorRef.current = {
      startedAtMs: performance.now(),
      startedSecond: initialSecond,
    }
    previewFrameRef.current = -1
    setSelectedSecondTransient(initialSecond)
    setIsPlaying(true)
  }

  const handlePause = () => {
    playbackAnchorRef.current = {
      startedAtMs: 0,
      startedSecond: clampedPlayhead,
    }
    previewFrameRef.current = -1
    setIsPlaying(false)
    setSelectedSecond(clampedPlayhead)
  }

  const handleReset = () => {
    playbackAnchorRef.current = {
      startedAtMs: 0,
      startedSecond: 0,
    }
    previewFrameRef.current = -1
    setIsPlaying(false)
    setSelectedSecond(0)
  }
  const handleTimelineChange = ([nextValue]) => {
    const nextSecond = clamp(nextValue, 0, totalDuration)
    setSelectedSecondTransient(nextSecond)
    previewFrameRef.current = -1

    if (isPlaying) {
      playbackAnchorRef.current = {
        startedAtMs: performance.now(),
        startedSecond: nextSecond,
      }
    }
  }

  const handleTimelineCommit = ([nextValue]) => {
    const nextSecond = clamp(nextValue, 0, totalDuration)
    previewFrameRef.current = -1
    setSelectedSecond(nextSecond)

    if (isPlaying) {
      playbackAnchorRef.current = {
        startedAtMs: performance.now(),
        startedSecond: nextSecond,
      }
    }
  }

  return (
    <div
      className={
        hasActivity
          ? 'shrink-0 border-border/70 bg-black/30 px-5 py-4 backdrop-blur-sm'
          : 'hidden'
      }
    >
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
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              className="rounded-xl"
              disabled={!hasActivity || !isPlaying}
              onClick={handlePause}
            >
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
          <span className="shrink-0 text-xs font-medium tabular-nums text-muted-foreground">
            {formatTimelineTime(clampedPlayhead)}
          </span>
          <div className="min-w-0 flex-1">
            <Slider
              min={0}
              max={Math.max(totalDuration, 1)}
              step={0.1}
              value={[clampedPlayhead]}
              disabled={!hasActivity}
              onValueChange={handleTimelineChange}
              onValueCommit={handleTimelineCommit}
            />
          </div>
          <span className="shrink-0 text-xs font-medium tabular-nums text-muted-foreground">
            {formatTimelineTime(totalDuration)}
          </span>
        </div>
      </div>
    </div>
  )
}

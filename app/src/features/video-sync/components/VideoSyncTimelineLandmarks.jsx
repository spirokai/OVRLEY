/**
 * Presentational landmark line and handle layer for the player timeline.
 */

import { SimpleTooltip } from '@/components/ui/simple-tooltip'

/**
 * Renders timeline landmark overlays. Idle offscreen indicators are explicitly
 * pointer-inert; captured in-view drags retain their active pointer handlers.
 *
 * @param {{ landmarks: object[] }} props Render-ready landmark models.
 * @returns {JSX.Element} Landmark overlay layer.
 */
export default function VideoSyncTimelineLandmarks({ landmarks }) {
  return (
    <>
      {landmarks.map((landmark) => {
        const Icon = landmark.Icon
        const handle = (
          <div
            className={`absolute -top-1 z-25 -translate-x-1/2 ${landmark.isInteractive ? 'pointer-events-auto' : 'pointer-events-none'}`}
            style={landmark.handleStyle}
          >
            {landmark.isInteractive ? (
              <button
                type="button"
                aria-label={`${landmark.label} landmark at ${landmark.videoSecond.toFixed(1)} seconds`}
                className={`flex h-6 w-6 cursor-grab items-center justify-center rounded-sm border border-background bg-background/95 p-0 shadow-sm outline-none active:cursor-grabbing focus-visible:ring-2 focus-visible:ring-primary/70 ${landmark.iconClassName}`}
                {...landmark.handleProps}
              >
                <Icon className="size-3.5" strokeWidth={2.5} aria-hidden="true" />
              </button>
            ) : (
              <div
                aria-hidden="true"
                className={`flex h-5 w-4 items-center justify-center rounded-sm border border-background bg-background/95 ${landmark.iconClassName}`}
              >
                <Icon className="size-3" strokeWidth={2.5} />
              </div>
            )}
          </div>
        )

        return (
          <div key={landmark.id} className="pointer-events-none absolute inset-0">
            <div
              className={`pointer-events-none absolute bottom-0 top-0 z-10 w-px -translate-x-1/2 ${landmark.lineClassName}`}
              style={landmark.lineStyle}
            />
            {landmark.isInteractive ? (
              <SimpleTooltip content={`${landmark.label} · ${landmark.videoSecond.toFixed(1)}s`}>{handle}</SimpleTooltip>
            ) : (
              handle
            )}
          </div>
        )
      })}
    </>
  )
}

/**
 * Presentational landmark line and handle layer for the player timeline.
 */

import { useTranslation } from 'react-i18next'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'

/**
 * Renders timeline landmark overlays. Idle offscreen indicators are explicitly
 * pointer-inert; captured in-view drags retain their active pointer handlers.
 *
 * @param {{ landmarks: object[] }} props Render-ready landmark models.
 * @returns {JSX.Element} Landmark overlay layer.
 */
export default function VideoSyncTimelineLandmarks({ landmarks }) {
  const { t } = useTranslation()

  return (
    <>
      {landmarks.map((landmark) => {
        const Icon = landmark.Icon
        const label = t(landmark.labelKey, landmark.defaultLabel)
        const timing = { label, seconds: landmark.videoSecond.toFixed(1) }
        const handle = (
          <div
            className={`absolute top-4 z-25 -translate-x-1/2 ${landmark.isInteractive ? 'pointer-events-auto' : 'pointer-events-none'}`}
            style={landmark.handleStyle}
          >
            {landmark.isInteractive ? (
              <SimpleTooltip content={t('videoSync.landmarkTooltip', '{{label}} - {{seconds}}s', timing)}>
                <button
                  type="button"
                  aria-label={t('videoSync.landmarkAriaLabel', '{{label}} landmark at {{seconds}} seconds', timing)}
                  className={`flex h-6 w-6 cursor-grab items-center justify-center rounded-sm border border-background p-0 text-background shadow-sm outline-none active:cursor-grabbing focus-visible:ring-2 focus-visible:ring-primary/70 ${landmark.lineClassName}`}
                  {...landmark.handleProps}
                >
                  <Icon className="size-3.5" strokeWidth={2.5} aria-hidden="true" />
                </button>
              </SimpleTooltip>
            ) : (
              <div
                aria-hidden="true"
                className={`flex h-5 w-4 items-center justify-center rounded-sm border border-background text-background ${landmark.lineClassName}`}
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
            {handle}
          </div>
        )
      })}
    </>
  )
}

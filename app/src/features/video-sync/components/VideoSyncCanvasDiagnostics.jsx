import { getInterpolatedActivityValue, getMetricSeries } from '@/features/overlay-editor/utils/overlayEditorUtils'
import { formatStandardMetricDisplay } from '@/features/widget-preview/widgets/metric/format'

const SPEED_FONT_FAMILY = 'JetBrains Mono'
const SPEED_FONT_SIZE = 126
const SPEED_UNITS_FONT_SIZE = 37
const SPEED_FORMAT = Object.freeze({ decimals: 1, display_unit: 'kmh', show_units: true })

/**
 * Places the current speed over the source video.
 *
 * @param {object} props Canvas inputs.
 * @param {number} props.displayScale Scale of the displayed video screen.
 * @returns {JSX.Element} Diagnostic canvas layer.
 */
export default function VideoSyncCanvasDiagnostics({ activity, displayScale, previewSecond }) {
  const speedSeries = getMetricSeries(activity, 'speed')
  const speed =
    Array.isArray(speedSeries) && speedSeries.some((value) => value !== null && value !== undefined)
      ? formatStandardMetricDisplay('speed', getInterpolatedActivityValue(activity, 'speed', previewSecond), SPEED_FORMAT)
      : null

  return (
    <div data-testid="video-sync-canvas-diagnostics" className="pointer-events-none absolute inset-0 z-40">
      <div data-testid="video-sync-speed-diagnostic" className="absolute bottom-[6%] right-[4%] whitespace-nowrap">
        {speed ? (
          <div className="flex items-baseline text-white" style={{ fontFamily: SPEED_FONT_FAMILY, gap: 8 * displayScale }}>
            <span style={{ fontSize: SPEED_FONT_SIZE * displayScale, lineHeight: 1 }}>{speed.value}</span>
            <span style={{ fontSize: SPEED_UNITS_FONT_SIZE * displayScale, lineHeight: 1 }}>{speed.units}</span>
          </div>
        ) : null}
      </div>
    </div>
  )
}

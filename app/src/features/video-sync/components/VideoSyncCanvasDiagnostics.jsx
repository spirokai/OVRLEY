import { useTranslation } from 'react-i18next'
import { WidgetPreview } from '@/features/widget-preview'

function UnavailableDiagnostic() {
  const { t } = useTranslation()
  return (
    <div className="flex h-full items-center justify-center rounded-sm border border-white/25 bg-black/70 text-lg font-semibold text-white/60 shadow-xl backdrop-blur-sm">
      {t('videoSync.unavailable', 'Unavailable')}
    </div>
  )
}

/**
 * Places fixed speed and route widget previews over the source video.
 *
 * @param {object} props Existing widget-preview inputs and synthetic models.
 * @returns {JSX.Element} Diagnostic canvas layer.
 */
export default function VideoSyncCanvasDiagnostics({ activity, exportRange, globalScale, previewSecond, sceneStyle, speed, route }) {
  return (
    <div data-testid="video-sync-canvas-diagnostics" className="pointer-events-none absolute inset-0 z-40">
      <div data-testid="video-sync-speed-diagnostic" className="absolute bottom-[6%] right-[4%] min-h-24 min-w-64">
        {speed.available ? (
          <WidgetPreview
            widget={speed.widget}
            activity={activity}
            previewSecond={previewSecond}
            globalOpacity={1}
            globalScale={globalScale}
            metricPreviewModel={speed.previewModel}
            sceneStyle={sceneStyle}
          />
        ) : (
          <UnavailableDiagnostic />
        )}
      </div>
      <div
        data-testid="video-sync-route-diagnostic"
        className="absolute right-[4%] top-[6%]"
        style={{ width: route.widget.data.width, height: route.widget.data.height }}
      >
        {route.available ? (
          <WidgetPreview
            widget={route.widget}
            activity={activity}
            previewSecond={previewSecond}
            globalOpacity={1}
            globalScale={globalScale}
            sceneStyle={sceneStyle}
            exportRange={exportRange}
          />
        ) : (
          <UnavailableDiagnostic />
        )}
      </div>
    </div>
  )
}

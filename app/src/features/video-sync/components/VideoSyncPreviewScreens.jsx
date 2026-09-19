import { MapPin } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { VideoPreviewSurface } from '@/features/video-preview'
import VideoSyncCanvasDiagnostics from './VideoSyncCanvasDiagnostics'
import useVideoSyncMap from '../hooks/useVideoSyncMap'
import { VIDEO_SYNC_MAP_STYLES, VIDEO_SYNC_PREVIEW_SCREEN_GAP } from '../data/videoSyncConstants'

function CourseLocationAction({ actionPoint, onConfirm }) {
  const { t } = useTranslation()
  const buttonLabel = t('videoSync.setLocationInMap', 'Set location in map')

  if (!actionPoint) return null
  return (
    <div
      className="absolute z-10 -translate-x-1/2 -translate-y-[calc(100%+8px)]"
      style={{ left: actionPoint.x, top: actionPoint.y }}
      onClick={(event) => event.stopPropagation()}
    >
      <Button
        type="button"
        onClick={onConfirm}
        variant="ghost"
        size="sm"
        className="h-7 w-auto justify-center border border-video-sync-location/70 bg-surface px-2 text-[10px] font-semibold text-video-sync-location hover:bg-surface-elevated hover:text-video-sync-location focus-visible:ring-video-sync-location/50"
      >
        <MapPin className="size-3.5 shrink-0" aria-hidden="true" />
        <span>{buttonLabel}</span>
      </Button>
    </div>
  )
}

function MapStyleSelector({ style, onStyleChange }) {
  const label = useTranslation().t('videoSync.mapStyle', 'Map style')
  return (
    <div className="absolute bottom-2 left-2 z-10" onClick={(event) => event.stopPropagation()}>
      <Select value={style} onValueChange={onStyleChange}>
        <SelectTrigger size="sm" className="h-7 w-28 bg-surface/95 px-2 text-[10px] shadow-md" aria-label={label}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {VIDEO_SYNC_MAP_STYLES.map((styleName) => (
            <SelectItem key={styleName} value={styleName}>
              {styleName.charAt(0).toUpperCase() + styleName.slice(1)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function VideoSyncMapPreview({ activity, detection, onSetCourseLocation }) {
  const { containerRef, style, onStyleChange, actionPoint, onConfirmActionPoint } = useVideoSyncMap({ activity, detection, onSetCourseLocation })

  return (
    <div className="relative isolate h-full w-full bg-surface-elevated" data-testid="maplibre-map">
      <div ref={containerRef} className="h-full w-full" />
      <MapStyleSelector style={style} onStyleChange={onStyleChange} />
      <CourseLocationAction actionPoint={actionPoint} onConfirm={onConfirmActionPoint} />
    </div>
  )
}

/**
 * Renders the equal-size video and map screens used only by the video-sync workspace.
 *
 * @param {object} props Component props.
 * @param {object|null} props.activity Canonical parsed activity.
 * @param {object|null} props.detection Canonical detected-event model.
 * @param {number} props.displayScale Shared scale applied to the complete pair.
 * @param {number} props.previewSecond Current activity preview second.
 * @param {((activitySecond: number) => void)|null} props.onSetCourseLocation Resolves the video location landmark to a course time.
 * @param {{width: number, height: number}} props.sceneSize Canonical screen dimensions.
 * @param {(element: HTMLElement|null) => void} props.setSceneElement Registers the scaled content for zoom anchoring.
 * @returns {JSX.Element} Video-sync preview pair.
 */
export default function VideoSyncPreviewScreens({ activity, detection, displayScale, onSetCourseLocation = null, previewSecond, sceneSize, setSceneElement }) {
  const screenWidth = sceneSize.width * displayScale
  const screenHeight = sceneSize.height * displayScale
  const screenGap = VIDEO_SYNC_PREVIEW_SCREEN_GAP * displayScale
  const screenStyle = { width: screenWidth, height: screenHeight }

  return (
    <div
      ref={setSceneElement}
      data-testid="video-sync-preview-screens"
      className="grid shrink-0"
      style={{ gridTemplateColumns: `repeat(2, ${screenWidth}px)`, width: screenWidth * 2 + screenGap, height: screenHeight, gap: screenGap }}
    >
      <div
        data-testid="video-sync-video-screen"
        className="relative shrink-0 overflow-hidden rounded-sm border border-border/50 bg-black shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)]"
        style={screenStyle}
      >
        <VideoPreviewSurface displayScale={1} isActive>
          <VideoSyncCanvasDiagnostics activity={activity} displayScale={displayScale} previewSecond={previewSecond} />
        </VideoPreviewSurface>
      </div>
      <div
        data-testid="video-sync-map-screen"
        className="relative shrink-0 overflow-hidden rounded-sm border border-border/50 bg-surface-elevated shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)]"
        style={screenStyle}
      >
        <VideoSyncMapPreview activity={activity} detection={detection} onSetCourseLocation={onSetCourseLocation} />
      </div>
    </div>
  )
}

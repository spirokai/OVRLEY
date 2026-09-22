import { MapPin } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { VideoPreviewSurface } from '@/features/video-preview'
import useVideoSyncPreview from '../hooks/useVideoSyncPreview'
import { VIDEO_SYNC_MAP_STYLES, VIDEO_SYNC_NAVIGATION_MAP_MAX_PITCH, VIDEO_SYNC_NAVIGATION_MAP_MIN_PITCH } from '../data/videoSyncConstants'

function CourseLocationAction({ actionPoint, onConfirm }) {
  const { t } = useTranslation()
  const buttonLabel = t('videoSync.setLocationInMap', 'Set location')

  if (!actionPoint) return null
  return (
    <div
      className="absolute z-10 -translate-x-1/2 -translate-y-[calc(100%+8px)] bg-surface rounded-sm shadow-md"
      style={{ left: actionPoint.x, top: actionPoint.y }}
      onClick={(event) => event.stopPropagation()}
    >
      <Button
        type="button"
        onClick={onConfirm}
        variant="ghost"
        size="sm"
        className="h-7 w-auto uppercase justify-center border border-video-sync-location bg-surface px-2 text-[0.75rem] font-semibold text-video-sync-location hover:text-video-sync-location hover:bg-video-sync-location/20 focus-visible:ring-video-sync-location/50"
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

function VideoSyncMapPreview({ containerRef, actionPoint, onConfirmActionPoint, onStyleChange, style }) {
  return (
    <div className="relative isolate h-full w-full bg-surface-elevated" data-testid="maplibre-map">
      <div ref={containerRef} className="h-full w-full" />
      <MapStyleSelector style={style} onStyleChange={onStyleChange} />
      <CourseLocationAction actionPoint={actionPoint} onConfirm={onConfirmActionPoint} />
    </div>
  )
}

function VideoSyncNavigationMap({ containerRef, pitch, onPitchChange }) {
  return (
    <div
      data-testid="video-sync-navigation-map"
      className="absolute top-[3%] right-[2%] z-20 isolate aspect-square w-[25%] overflow-hidden rounded-sm border border-white/50 bg-surface-elevated shadow-lg"
      onWheel={(event) => event.stopPropagation()}
    >
      <div ref={containerRef} className="h-full w-full" aria-label="Route navigation map" />
      <div className="absolute top-1/2 left-[3%] z-10 flex h-[68%] -translate-y-1/2 flex-col items-center gap-1 rounded-full px-1.5 py-1.5 ">
        <Slider
          aria-label="Navigation map pitch"
          className="h-full data-[orientation=vertical]:min-h-0"
          max={VIDEO_SYNC_NAVIGATION_MAP_MAX_PITCH}
          min={VIDEO_SYNC_NAVIGATION_MAP_MIN_PITCH}
          onValueChange={([nextPitch]) => onPitchChange(nextPitch)}
          orientation="vertical"
          step={1}
          value={[pitch]}
        />
      </div>
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
 * @param {(activitySecond: number) => void} props.onSetCourseLocation Resolves the video location landmark to a course time.
 * @param {() => void} props.onDeleteCourseLocation Clears the selected course location.
 * @param {{width: number, height: number}} props.sceneSize Canonical screen dimensions.
 * @param {(element: HTMLElement|null) => void} props.setSceneElement Registers the scaled content for zoom anchoring.
 * @returns {JSX.Element} Video-sync preview pair.
 */
export default function VideoSyncPreviewScreens({
  activity,
  detection,
  displayScale,
  onSetCourseLocation,
  onDeleteCourseLocation,
  previewSecond,
  sceneSize,
  setSceneElement,
}) {
  const { selectionMapRef, navigationMapRef, actionPoint, onConfirmActionPoint, pitch, onPitchChange, screenLayout, speed, style, onStyleChange } =
    useVideoSyncPreview({
      activity,
      detection,
      displayScale,
      onSetCourseLocation,
      onDeleteCourseLocation,
      previewSecond,
      sceneSize,
    })

  return (
    <div ref={setSceneElement} data-testid="video-sync-preview-screens" className="grid shrink-0" style={screenLayout.pairStyle}>
      <div
        data-testid="video-sync-video-screen"
        className="relative z-0 isolate shrink-0 overflow-hidden rounded-sm border border-border/50 bg-black shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)]"
        style={screenLayout.screenStyle}
      >
        <VideoPreviewSurface displayScale={1} isActive>
          <div data-testid="video-sync-canvas-diagnostics" className="pointer-events-none absolute inset-0 z-40">
            <div data-testid="video-sync-speed-diagnostic" className="absolute top-[6%] left-[4%] whitespace-nowrap">
              {speed ? (
                <div className="flex items-baseline text-white" style={{ fontFamily: 'JetBrains Mono', gap: 8 * displayScale }}>
                  <span style={{ fontSize: 126 * displayScale, lineHeight: 1 }}>{speed.value}</span>
                  <span style={{ fontSize: 37 * displayScale, lineHeight: 1 }}>{speed.units}</span>
                </div>
              ) : null}
            </div>
          </div>
        </VideoPreviewSurface>
        <VideoSyncNavigationMap containerRef={navigationMapRef} pitch={pitch} onPitchChange={onPitchChange} />
      </div>
      <div
        data-testid="video-sync-map-screen"
        className="relative shrink-0 overflow-hidden rounded-sm border border-border/50 bg-surface-elevated shadow-[0_5px_20px_3px_rgba(0,0,0,0.2)]"
        style={screenLayout.screenStyle}
      >
        <VideoSyncMapPreview
          containerRef={selectionMapRef}
          actionPoint={actionPoint}
          onConfirmActionPoint={onConfirmActionPoint}
          onStyleChange={onStyleChange}
          style={style}
        />
      </div>
    </div>
  )
}

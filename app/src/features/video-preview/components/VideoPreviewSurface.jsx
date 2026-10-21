import { useRef } from 'react'
import { AlertTriangle, ExternalLink, Loader2, Scissors } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import useStore from '@/store/useStore'
import { useVideoPreview } from '../hooks/useVideoPreview'
import { useTransitionProxyQueue } from '../hooks/useTransitionProxies'
import { useTransitionProxyPlayback } from '../hooks/useTransitionProxyPlayback'

function HevcPlaybackPlaceholder({ displayScale, importId, openVideoPreviewHelp, platformOs, videoPreviewHelpAvailable }) {
  const { t } = useTranslation()
  const isWindows = platformOs === 'windows'
  const isLinux = platformOs === 'linux'

  return (
    <div
      key={`${importId ?? 'no-video'}-hevc-placeholder`}
      className="pointer-events-none absolute inset-0 flex items-center justify-center bg-black/90"
    >
      <div
        className="pointer-events-auto flex flex-col items-center gap-4 p-8 text-center"
        style={{
          transform: `scale(${0.5 + 0.5 / displayScale})`,
        }}
      >
        <AlertTriangle className="h-12 w-24 text-amber-400" />
        <div className="space-y-2">
          <p className="text-base font-medium text-amber-100">{t('overlay-editor.hevcVideoCannotBePlayed', 'HEVC video cannot be played')}</p>
          <p className="max-w-lg text-sm text-amber-200/80">
            {t(
              'overlay-editor.errors.hevcCodecMissing',
              'The system preview player could not play the video. Your system is likely missing the HEVC codec.',
            )}
          </p>
        </div>
        {isWindows && videoPreviewHelpAvailable ? (
          <div className="max-w-lg space-y-4 text-sm text-amber-200/80">
            <p>{t('overlay-editor.microsoftStoreHevcCodec', 'You can get the HEVC codec from the Microsoft Store.')}</p>
            <button
              type="button"
              className="cursor-pointer inline-flex items-center gap-2 rounded-xs bg-amber-500/20 px-4 py-2 text-sm font-semibold text-amber-200 transition-colors hover:bg-amber-500/30"
              onClick={openVideoPreviewHelp}
            >
              {t('overlay-editor.getHevcCodec', 'Get HEVC codec')}
              <ExternalLink className="h-4 w-4" />
            </button>
          </div>
        ) : null}
        {isLinux ? (
          <div className="max-w-lg space-y-4 text-left text-sm text-amber-200/80">
            <p>{t('overlay-editor.installCodecForSystem', 'Install the correct package for your system.')}</p>
            <p>{t('overlay-editor.onUbuntu', 'On Ubuntu:')}</p>
            <code className="block rounded bg-black/50 px-3 py-2 font-mono text-xs text-amber-100">sudo apt install restricted-extras</code>
          </div>
        ) : null}
      </div>
    </div>
  )
}

/**
 * Renders the shared video preview surface used by the editor and video-sync workspace.
 *
 * @param {object} props Component props.
 * @param {React.ReactNode} [props.children] Optional diagnostic layer.
 * @param {number} props.displayScale Current display scale.
 * @param {boolean} props.isActive Whether video playback synchronization is active.
 * @returns {JSX.Element} Video preview surface.
 */
export default function VideoPreviewSurface({ children, displayScale, isActive }) {
  const firstVideoRef = useRef(null)
  const secondVideoRef = useRef(null)
  const proxyVideoRef = useRef(null)
  const isVideoMuted = useStore((state) => state.isVideoMuted)
  const platformOs = useStore((state) => state.platformOs)
  const { t } = useTranslation()
  const slotRefs = [firstVideoRef, secondVideoRef]
  const {
    slots,
    videoSrc,
    importId,
    isOutOfRange,
    isDeleted,
    isInOverlap,
    transition,
    hevcPlaybackWarning,
    openVideoPreviewHelp,
    videoPreviewHelpAvailable,
  } = useVideoPreview(slotRefs, isActive)
  useTransitionProxyQueue()
  useTransitionProxyPlayback(proxyVideoRef, transition)
  const hasHevcPlaybackError = Boolean(hevcPlaybackWarning)
  // Inside a transition window the proxy shows the blend and plays its crossfaded audio.
  const showsProxy = transition?.proxyUrl != null

  return (
    <>
      {/* Both elements stay mounted; the parked one is invisible and silent until a cut swaps them. */}
      {slots.map((slot, index) =>
        slot.src && !(slot.isActive && hasHevcPlaybackError) ? (
          <video
            key={index}
            ref={slotRefs[index]}
            src={slot.src}
            data-slot={slot.isActive ? 'active' : 'parked'}
            className={cn(
              'pointer-events-none absolute inset-0 h-full w-full object-cover',
              !slot.isActive ? 'opacity-0' : isOutOfRange ? 'opacity-20' : 'opacity-100',
            )}
            preload={slot.isActive ? 'metadata' : 'auto'}
            playsInline
            muted={!slot.isActive || isVideoMuted || showsProxy}
            onError={(event) => console.error('[VideoPreviewSurface] Video Error:', event)}
          />
        ) : null,
      )}
      {showsProxy ? (
        <video
          ref={proxyVideoRef}
          src={transition.proxyUrl}
          data-slot="transition"
          className="pointer-events-none absolute inset-0 h-full w-full object-cover"
          preload="auto"
          playsInline
          muted={isVideoMuted}
        />
      ) : null}
      {children}
      {transition !== null && !showsProxy && transition.proxyStatus !== 'failed' ? (
        <div className="pointer-events-none absolute right-0 top-0 z-30 p-4">
          <div role="status" className="flex items-center gap-2 rounded-sm bg-black/70 px-3 py-1.5 text-xs font-semibold text-white">
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
            {t('video-preview.renderingTransition', 'Rendering transition preview…')}
          </div>
        </div>
      ) : null}
      {videoSrc && isDeleted ? (
        <div className="pointer-events-none absolute inset-x-0 top-0 z-30 flex justify-center p-4">
          <div role="status" className="flex items-center gap-2 rounded-sm bg-black/80 px-4 py-2 text-sm font-semibold text-white shadow-lg">
            <Scissors className="h-4 w-4 shrink-0" aria-hidden="true" />
            {t('video-preview.deletedPartIndicator', 'Deleted part – not exported')}
          </div>
        </div>
      ) : null}
      {videoSrc && isInOverlap ? (
        <div className="pointer-events-none absolute inset-x-0 bottom-0 z-30 flex justify-center p-4">
          <div
            role="alert"
            className="flex max-w-[90%] items-center gap-3 rounded-sm border-2 border-destructive bg-destructive/90 px-5 py-3 text-base font-bold text-white shadow-2xl"
          >
            <AlertTriangle className="h-6 w-6 shrink-0" aria-hidden="true" />
            {t(
              'video-preview.overlapPlaybackWarning',
              'Videos overlap here. The preview plays the earlier video first; one combined export is not possible.',
            )}
          </div>
        </div>
      ) : null}
      {videoSrc && hasHevcPlaybackError ? (
        <HevcPlaybackPlaceholder
          displayScale={displayScale}
          importId={importId}
          openVideoPreviewHelp={openVideoPreviewHelp}
          platformOs={platformOs}
          videoPreviewHelpAvailable={videoPreviewHelpAvailable}
        />
      ) : null}
    </>
  )
}

import { useRef } from 'react'
import { AlertTriangle, ExternalLink } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import useStore from '@/store/useStore'
import { useVideoPreview } from '../hooks/useVideoPreview'

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
  const videoRef = useRef(null)
  const isVideoMuted = useStore((state) => state.isVideoMuted)
  const platformOs = useStore((state) => state.platformOs)
  const { videoSrc, importId, isOutOfRange, hevcPlaybackWarning, openVideoPreviewHelp, videoPreviewHelpAvailable } = useVideoPreview(
    videoRef,
    isActive,
  )
  const hasHevcPlaybackError = Boolean(hevcPlaybackWarning)

  return (
    <>
      {videoSrc && !hasHevcPlaybackError ? (
        <video
          key={importId ?? 'no-video'}
          ref={videoRef}
          src={videoSrc}
          className={cn('pointer-events-none absolute inset-0 h-full w-full object-cover', isOutOfRange ? 'opacity-20' : 'opacity-100')}
          preload="metadata"
          playsInline
          muted={isVideoMuted}
          onError={(event) => console.error('[VideoPreviewSurface] Video Error:', event)}
        />
      ) : null}
      {children}
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

/**
 * Composes preview-video source resolution, warning lifecycle, scrub
 * scheduling, and playback synchronization around two <video> elements: the
 * active one shows the piece at the playhead, the parked one waits on the next
 * piece so a cut swaps elements instead of reloading one.
 */

import { useCallback, useEffect, useRef, useState } from 'react'
import { openHevcSupport } from '@/api/backend'
import { useTransitionProxyJobs, useVideoTimeline } from '@/hooks/useVideoTimeline'
import { framesToSeconds } from '@/lib/video-program/frame-grid'
import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { assignPreviewSlots, EMPTY_PREVIEW_SLOTS } from '@/lib/video-program/preview-slots'
import { listShownTransitionJoinKeys, resolveContinuation, resolveTransitionWindow } from '@/lib/video-program/transition-proxies'
import useStore from '@/store/useStore'
import { useVideoPlaybackClock } from './useVideoPlaybackClock'
import { useVideoPreviewWarnings } from './useVideoPreviewWarnings'
import { SCRUB_SEEK_EPSILON_SECONDS, SCRUB_SEEK_INTERVAL_MS } from '../data/videoPreviewConstants'
import { primeVideoFirstFrame, syncVideoCurrentTime } from '../utils/videoPreviewPlayback'
import { createVideoPreviewScrubScheduler } from '../utils/videoPreviewScrubScheduler'
import { resolveVideoPreviewSource } from '../utils/videoPreviewSource'
import i18next from 'i18next'

function previewSourceOf(clip) {
  return resolveVideoPreviewSource({ importedVideoPath: clip.path, importedVideoPreviewUrl: clip.preview.url })
}

/**
 * Manages the two preview video elements and their synchronization with the global playhead.
 *
 * @param {Array<React.RefObject<HTMLVideoElement>>} slotRefs - Refs to the two video elements.
 * @param {boolean} isActive - Whether the imported video preview is currently visible.
 * @returns {{ slots: Array<{src: string, isActive: boolean}>, videoSrc: string, importId: string|null, isOutOfRange: boolean, isDeleted: boolean, isInOverlap: boolean, transition: {proxyStatus: 'queued'|'rendering'|'ready'|'failed', proxyUrl: string|null, elapsedSeconds: number, isPlaying: boolean}|null, openVideoPreviewHelp: Function, videoPreviewHelpAvailable: boolean, videoPreviewMessages: string[] }}
 *   Inside a transition window `transition` describes its proxy, shown over the outgoing piece once ready.
 */
export function useVideoPreview(slotRefs, isActive = true) {
  // Store selectors - subscribes to the displayed clips, playhead position, playback mode, and drag state.
  const { clips, program } = useVideoTimeline().displayed
  const isDragPreview = useStore((state) => state.videoDragPreview !== null)
  const platformOs = useStore((state) => state.platformOs)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const previewPlaybackState = useStore((state) => state.previewPlaybackState)
  const previewPlaybackSource = useStore((state) => state.previewPlaybackSource)
  const pausePreviewPlayback = useStore((state) => state.pausePreviewPlayback)
  const setSelectedSecond = useStore((state) => state.setSelectedSecond)
  const startPreviewPlayback = useStore((state) => state.startPreviewPlayback)
  const transitionProxies = useStore((state) => state.transitionProxies)
  const transitionProxyJobs = useTransitionProxyJobs()

  // Derived state - the clip at the playhead (or the upcoming/last clip outside clips) owns the video element.
  const target = resolvePreviewTarget(program, selectedSecond)
  const activeClip = target === null ? null : clips.find((clip) => clip.id === target.videoId)
  const videoOffsetSeconds = activeClip?.offsetSeconds ?? 0
  const desiredVideoSecond = target?.sourceSecond ?? 0
  const playingPieceId = target?.isInside ? target.piece.id : null
  const pieceEndSourceSecond = target?.isInside ? target.piece.sourceEndSecond : null
  const activePath = activeClip?.path ?? null
  const activePreviewUrl = activeClip?.preview.url ?? null
  const isVideoPlaybackMode = isActive && previewPlaybackState === 'playing' && previewPlaybackSource === 'video' && !isDragPreview
  const videoSrc = activePath === null ? '' : resolveVideoPreviewSource({ importedVideoPath: activePath, importedVideoPreviewUrl: activePreviewUrl })

  // Element slots - the active element shows the target piece; the parked one waits on the piece after it.
  const shownJoinKeys = listShownTransitionJoinKeys(transitionProxyJobs, transitionProxies)
  const continuation = target === null ? null : resolveContinuation(program, target.piece.id, shownJoinKeys)
  const preloadPiece = continuation?.piece ?? null
  const preloadClip = preloadPiece === null ? null : clips.find((clip) => clip.id === preloadPiece.videoId)
  const [previousSlots, setPreviousSlots] = useState(EMPTY_PREVIEW_SLOTS)
  const slotState = assignPreviewSlots(previousSlots, {
    active: videoSrc ? { src: videoSrc, pieceId: target.piece.id } : null,
    preload: preloadClip ? { src: previewSourceOf(preloadClip), pieceId: preloadPiece.id } : null,
  })
  if (slotState !== previousSlots) setPreviousSlots(slotState)
  const videoRef = slotRefs[slotState.activeIndex]
  const parkedRef = slotRefs[1 - slotState.activeIndex]
  const parkedSlot = slotState.slots[1 - slotState.activeIndex]
  // The next piece continues after the start its incoming transition already showed.
  const parkedSecond =
    parkedSlot !== null && parkedSlot.pieceId === preloadPiece?.id
      ? preloadPiece.sourceStartSecond + continuation.second - preloadPiece.timelineStartSecond
      : null

  // Warning lifecycle - tracks metadata, native-player, and slow-seek messages separately from playback sync.
  const { hevcPlaybackWarning, metadataStatusMessage, nativeVideoError, seekWarning } = useVideoPreviewWarnings({
    codecName: activeClip?.codecName ?? null,
    isActive,
    videoRef,
    videoSrc,
  })

  const openVideoPreviewHelp = useCallback(() => {
    openHevcSupport().catch((error) => {
      console.error('[useVideoPreview] Failed to open HEVC help', error)
    })
  }, [])

  // Piece end - continue with the next piece (skipping gaps, deleted parts, and the start a shown
  // transition already played) or hand the rest of the activity to the timeline clock, holding the
  // last frame of the piece.
  const handlePieceEnded = () => {
    const playingPiece = program.pieces.find((piece) => piece.id === playingPieceId)
    const next = resolveContinuation(program, playingPieceId, shownJoinKeys)
    if (next !== null) {
      startPreviewPlayback({ source: 'video', second: next.second })
      return
    }
    const video = videoRef.current
    if (video) {
      video.pause()
      syncVideoCurrentTime(video, framesToSeconds(playingPiece.sourceEndFrame - 1, playingPiece.fps))
    }
    startPreviewPlayback({ source: 'timeline', second: playingPiece.timelineEndSecond })
  }

  // Video playback clock - publishes preview time from the video element while playing.
  useVideoPlaybackClock({
    videoRef,
    isActive: Boolean(videoSrc) && isVideoPlaybackMode,
    videoFps: activeClip?.fps ?? null,
    videoOffsetSeconds,
    pieceEndSourceSecond,
    onPreviewSecond: setSelectedSecond,
    onPlaybackEnded: handlePieceEnded,
  })

  const scrubSchedulerRef = useRef(null)
  const videoPlaybackOwnerRef = useRef(null)

  // Drag preview - seek the video directly from the transient offset without committing global sync state.
  useEffect(() => {
    if (!isDragPreview) {
      return
    }

    const video = videoRef.current
    if (!video || !videoSrc) {
      return
    }

    if (!video.paused) {
      video.pause()
    }

    scrubSchedulerRef.current?.schedule(desiredVideoSecond)
  }, [desiredVideoSecond, isDragPreview, videoRef, videoSrc])

  // Scrub scheduler ownership - preserve throttle state across playhead updates.
  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) {
      return undefined
    }

    const scrubScheduler = createVideoPreviewScrubScheduler({
      epsilonSeconds: SCRUB_SEEK_EPSILON_SECONDS,
      flushIntervalMs: SCRUB_SEEK_INTERVAL_MS,
      video,
    })
    scrubSchedulerRef.current = scrubScheduler

    return () => {
      scrubScheduler.clear()
      if (scrubSchedulerRef.current === scrubScheduler) {
        scrubSchedulerRef.current = null
      }
    }
  }, [isActive, videoRef, videoSrc])

  // Video sync - the video clock owns playback; external playhead updates own paused and scrubbed seeks.
  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) {
      return undefined
    }

    const syncPlaybackState = () => {
      if (isDragPreview) {
        videoPlaybackOwnerRef.current = null
        return
      }

      const handlePlaybackFailure = (error) => {
        const currentState = useStore.getState()
        const isCurrentVideoPlayback =
          videoRef.current === video && currentState.previewPlaybackState === 'playing' && currentState.previewPlaybackSource === 'video'

        if (!isCurrentVideoPlayback) {
          return
        }

        if (error?.name !== i18next.t('video-preview.aborterror', 'AbortError') && error?.name !== 'NotAllowedError') {
          console.error('[useVideoPreview] Failed to start playback', error)
        }

        pausePreviewPlayback(currentState.selectedSecond)
      }

      const startVideoPlayback = () => {
        const playPromise = video.play()

        if (playPromise && typeof playPromise.catch === 'function') {
          playPromise.catch(handlePlaybackFailure)
        }
      }

      if (isVideoPlaybackMode) {
        scrubSchedulerRef.current?.clear()
        // A new piece (also of the same video, after a deleted part) or a moved clip restarts from the playhead.
        const owner = videoPlaybackOwnerRef.current
        if (
          owner?.video !== video ||
          owner.videoSrc !== videoSrc ||
          owner.videoOffsetSeconds !== videoOffsetSeconds ||
          owner.pieceId !== playingPieceId
        ) {
          syncVideoCurrentTime(video, desiredVideoSecond)
          videoPlaybackOwnerRef.current = { video, videoSrc, videoOffsetSeconds, pieceId: playingPieceId }
        }

        if (video.paused) {
          startVideoPlayback()
        }

        return
      }

      videoPlaybackOwnerRef.current = null
      if (!video.paused) {
        video.pause()
      }

      if (previewPlaybackState === 'playing') {
        // Timeline-owned playback leaves the frame restored by the video clock untouched.
        scrubSchedulerRef.current?.clear()
        return
      }

      if (previewPlaybackState === 'scrubbing') {
        scrubSchedulerRef.current?.schedule(desiredVideoSecond)
        return
      }

      scrubSchedulerRef.current?.clear()
      syncVideoCurrentTime(video, desiredVideoSecond)
    }

    const handleLoadedMetadata = () => {
      syncPlaybackState()
      primeVideoFirstFrame(video)
    }

    syncPlaybackState()
    video.addEventListener('loadedmetadata', handleLoadedMetadata)

    return () => {
      video.removeEventListener('loadedmetadata', handleLoadedMetadata)
    }
  }, [
    desiredVideoSecond,
    isActive,
    isDragPreview,
    isVideoPlaybackMode,
    pausePreviewPlayback,
    playingPieceId,
    previewPlaybackState,
    videoOffsetSeconds,
    videoRef,
    videoSrc,
  ])

  // Parked element - stays paused on the first frame of the next piece, ready to take over at the cut.
  useEffect(() => {
    const video = parkedRef.current
    if (!video || parkedSlot === null) {
      return undefined
    }

    const park = () => {
      if (!video.paused) {
        video.pause()
      }
      if (parkedSecond !== null) {
        syncVideoCurrentTime(video, parkedSecond)
      }
    }
    const handleLoadedMetadata = () => {
      park()
      primeVideoFirstFrame(video)
    }

    park()
    video.addEventListener('loadedmetadata', handleLoadedMetadata)
    return () => {
      video.removeEventListener('loadedmetadata', handleLoadedMetadata)
    }
  }, [parkedRef, parkedSecond, parkedSlot])

  // Transition window - its proxy (once rendered) covers the end of the outgoing piece.
  const transitionWindow = isDragPreview ? null : resolveTransitionWindow(program, selectedSecond)
  const transitionJob = transitionWindow === null ? null : transitionProxyJobs.find((job) => job.joinKey === transitionWindow.join.key)
  const transitionProxy = transitionJob === undefined || transitionJob === null ? null : (transitionProxies[transitionJob.key] ?? null)
  const transition =
    transitionWindow === null
      ? null
      : {
          proxyStatus: transitionProxy?.status ?? 'queued',
          proxyUrl: transitionProxy?.status === 'ready' ? transitionProxy.url : null,
          elapsedSeconds: transitionWindow.elapsedSeconds,
          isPlaying: isVideoPlaybackMode,
        }

  // Derived return values - aggregate imported-video warnings with local preview messages.
  const isOutOfRange = target === null || !target.isInside
  const isInOverlap = program.overlaps.some((overlap) => selectedSecond >= overlap.startSecond && selectedSecond < overlap.endSecond)
  const videoPreviewHelpAvailable = Boolean(hevcPlaybackWarning) && platformOs === 'windows'
  const videoPreviewMessages = [
    hevcPlaybackWarning,
    ...(activeClip?.preview.warnings ?? []),
    metadataStatusMessage,
    seekWarning,
    nativeVideoError,
  ].filter(Boolean)

  return {
    slots: slotState.slots.map((slot, index) => ({ src: slot?.src ?? '', isActive: index === slotState.activeIndex })),
    videoSrc,
    importId: activeClip?.preview.importId ?? null,
    isOutOfRange,
    isDeleted: target?.isDeleted ?? false,
    isInOverlap,
    transition,
    hevcPlaybackWarning,
    openVideoPreviewHelp,
    videoPreviewHelpAvailable,
    videoPreviewMessages,
  }
}

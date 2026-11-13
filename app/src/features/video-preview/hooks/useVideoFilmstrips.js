/**
 * Keeps the timeline filmstrips of the collection's found videos: every video's base strip, and the zoom windows the
 * timeline currently wants.
 */

import { useEffect } from 'react'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'
import { FILMSTRIP_CONCURRENCY, FILMSTRIP_WINDOWS_KEPT } from '../data/filmstrip'
import { BASE_FILMSTRIP_WINDOW, filmstripFrameSize, filmstripHwaccel, filmstripWindowKey } from '../utils/filmstrip'

/** Decode priority: base strips before zoom windows. */
const BASE_PRIORITY = 0
const WINDOW_PRIORITY = 1

let lastJobId = 0
let busySlots = 0
/** Jobs waiting for a decode slot, served by priority, then in the order they asked. */
const waiting = []

/** Waits for one of the `FILMSTRIP_CONCURRENCY` decode slots; resolves with the function that frees it. */
function acquireDecodeSlot(priority) {
  return new Promise((resolve) => {
    waiting.push({ priority, order: ++lastJobId, resolve })
    grantDecodeSlots()
  })
}

function grantDecodeSlots() {
  waiting.sort((a, b) => a.priority - b.priority || a.order - b.order)
  while (busySlots < FILMSTRIP_CONCURRENCY && waiting.length > 0) {
    busySlots += 1
    let released = false
    waiting.shift().resolve(() => {
      if (released) return
      released = true
      busySlots -= 1
      grantDecodeSlots()
    })
  }
}

/** A short `blob:` URL for the sprite: the timeline repeats it on every tile. */
function spriteObjectUrl(jpegBase64) {
  const bytes = Uint8Array.from(atob(jpegBase64), (character) => character.charCodeAt(0))
  return URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }))
}

function readyStrip(strip) {
  return {
    status: 'ready',
    spriteUrl: spriteObjectUrl(strip.spriteJpegBase64),
    startSeconds: strip.startSeconds,
    spacingSeconds: strip.spacingSeconds,
    sampleCount: strip.sampleCount,
  }
}

function isCurrentJob(path, key, jobId) {
  const strip = useStore.getState().videoFilmstrips[path]?.[key]
  return strip?.status === 'generating' && strip.jobId === jobId
}

function isWantedWindow(path, key) {
  return (useStore.getState().wantedFilmstripWindows[path] ?? []).some((window) => filmstripWindowKey(window) === key)
}

/**
 * Loads one strip: from the disk cache when it is there, otherwise decoded in a slot. A zoom window the timeline stopped
 * wanting while it waited for its slot is dropped unstarted, so it can be asked for again.
 */
function startStrip(video, window) {
  const { setVideoFilmstrip } = useStore.getState()
  const key = filmstripWindowKey(window)
  const jobId = ++lastJobId
  const request = { path: video.path, durationSeconds: video.durationSeconds, window, ...filmstripFrameSize(video) }
  setVideoFilmstrip(video.path, key, { status: 'generating', jobId })
  const settle = (strip) => {
    // A video removed meanwhile, or a strip dropped meanwhile, discards the result.
    if (!isCurrentJob(video.path, key, jobId)) return
    setVideoFilmstrip(video.path, key, strip === null ? { status: 'failed' } : readyStrip(strip))
  }
  const decode = async () => {
    const cached = await backend.getCachedVideoFilmstrip(request)
    if (cached !== null) return settle(cached)
    const release = await acquireDecodeSlot(window.kind === 'base' ? BASE_PRIORITY : WINDOW_PRIORITY)
    try {
      if (!isCurrentJob(video.path, key, jobId)) return
      if (window.kind === 'zoom' && !isWantedWindow(video.path, key)) {
        useStore.getState().removeVideoFilmstrip(video.path, key)
        return
      }
      const codecs = await useStore.getState().fetchAvailableCodecs()
      settle(await backend.getVideoFilmstrip({ ...request, hwaccel: filmstripHwaccel(codecs) }))
    } finally {
      release()
    }
  }
  decode().catch((error) => {
    console.error('[useVideoFilmstripQueue] Filmstrip failed', error)
    if (isCurrentJob(video.path, key, jobId)) setVideoFilmstrip(video.path, key, { status: 'failed' })
  })
}

function dropStrip(path, key, strip) {
  useStore.getState().removeVideoFilmstrip(path, key)
  if (strip.status === 'ready') URL.revokeObjectURL(strip.spriteUrl)
}

/** Past `FILMSTRIP_WINDOWS_KEPT` loaded zoom windows, drops the ones the timeline does not show. */
function dropUnshownWindows(current, wanted) {
  const unshown = []
  let loaded = 0
  for (const [path, strips] of Object.entries(current)) {
    const shown = new Set((wanted[path] ?? []).map(filmstripWindowKey))
    for (const [key, strip] of Object.entries(strips)) {
      if (key === 'base' || strip.status !== 'ready') continue
      loaded += 1
      if (!shown.has(key)) unshown.push([path, key, strip])
    }
  }
  for (const [path, key, strip] of unshown.slice(0, Math.max(0, loaded - FILMSTRIP_WINDOWS_KEPT))) dropStrip(path, key, strip)
}

/**
 * Loads a base strip for every found video of the collection and the zoom windows the timeline wants, and forgets the
 * strips of videos that left the collection. Base strips decode before zoom windows. Missing videos have none. A failed
 * strip is not retried while its video stays; its tiles fall back to a coarser strip, or the clip shows plain.
 */
export function useVideoFilmstripQueue() {
  const importedVideos = useStore((state) => state.importedVideos)
  const filmstrips = useStore((state) => state.videoFilmstrips)
  const wantedWindows = useStore((state) => state.wantedFilmstripWindows)

  useEffect(() => {
    // The live state, not the rendered snapshot: an effect run twice (StrictMode) must not start a job twice.
    const { removeVideoFilmstrips, videoFilmstrips: current, wantedFilmstripWindows: wanted } = useStore.getState()
    const videos = new Map(importedVideos.filter((video) => !video.missing).map((video) => [video.path, video]))
    for (const [path, strips] of Object.entries(current)) {
      if (videos.has(path)) continue
      removeVideoFilmstrips(path)
      for (const strip of Object.values(strips)) if (strip.status === 'ready') URL.revokeObjectURL(strip.spriteUrl)
    }
    for (const [path, video] of videos) {
      if (!current[path]?.base) startStrip(video, BASE_FILMSTRIP_WINDOW)
    }
    for (const [path, windows] of Object.entries(wanted)) {
      const video = videos.get(path)
      if (!video) continue
      for (const window of windows) {
        if (!current[path]?.[filmstripWindowKey(window)]) startStrip(video, window)
      }
    }
    dropUnshownWindows(useStore.getState().videoFilmstrips, wanted)
  }, [importedVideos, filmstrips, wantedWindows])
}

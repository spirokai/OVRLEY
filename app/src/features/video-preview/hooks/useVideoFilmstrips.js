/**
 * Keeps a timeline filmstrip for every found video of the collection.
 */

import { useEffect } from 'react'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'
import { FILMSTRIP_CONCURRENCY } from '../data/filmstrip'
import { filmstripHwaccel, planFilmstrip } from '../utils/filmstrip'

let lastJobId = 0

/** A short `blob:` URL for the sprite sheet: the timeline repeats it on every tile. */
function spriteObjectUrl(jpegBase64) {
  const bytes = Uint8Array.from(atob(jpegBase64), (character) => character.charCodeAt(0))
  return URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }))
}

function isCurrentJob(path, jobId) {
  const filmstrip = useStore.getState().videoFilmstrips[path]
  return filmstrip?.status === 'generating' && filmstrip.jobId === jobId
}

async function decodeFilmstrip(path, plan) {
  const codecs = await useStore.getState().fetchAvailableCodecs()
  return backend.getVideoFilmstrip({ path, ...plan, hwaccel: filmstripHwaccel(codecs) })
}

function startFilmstrip(video) {
  const { setVideoFilmstrip } = useStore.getState()
  const jobId = ++lastJobId
  const plan = planFilmstrip(video)
  setVideoFilmstrip(video.path, { status: 'generating', jobId })
  void decodeFilmstrip(video.path, plan)
    .then((sheet) => {
      // A video removed meanwhile discards its filmstrip.
      if (!isCurrentJob(video.path, jobId)) return
      if (sheet === null) {
        setVideoFilmstrip(video.path, { status: 'failed' })
        return
      }
      setVideoFilmstrip(video.path, {
        status: 'ready',
        spriteUrl: spriteObjectUrl(sheet.spriteJpegBase64),
        columns: sheet.columns,
        rows: sheet.rows,
        intervalSeconds: plan.intervalSeconds,
        sampleCount: plan.sampleCount,
      })
    })
    .catch((error) => {
      console.error('[useVideoFilmstripQueue] Filmstrip failed', error)
      if (isCurrentJob(video.path, jobId)) setVideoFilmstrip(video.path, { status: 'failed' })
    })
}

/**
 * Decodes a filmstrip for every found video of the collection, a few at a time in collection order, and forgets the
 * filmstrips of videos that left it. Missing videos have none. A failed filmstrip is not retried while its video
 * stays; its clips show plain.
 */
export function useVideoFilmstripQueue() {
  const importedVideos = useStore((state) => state.importedVideos)
  const filmstrips = useStore((state) => state.videoFilmstrips)

  useEffect(() => {
    // The live state, not the rendered snapshot: an effect run twice (StrictMode) must not start a job twice.
    const { removeVideoFilmstrip, videoFilmstrips: current } = useStore.getState()
    const wanted = new Map(importedVideos.filter((video) => !video.missing).map((video) => [video.path, video]))
    for (const [path, filmstrip] of Object.entries(current)) {
      if (wanted.has(path)) continue
      removeVideoFilmstrip(path)
      if (filmstrip.status === 'ready') URL.revokeObjectURL(filmstrip.spriteUrl)
    }
    let generating = [...wanted.keys()].filter((path) => current[path]?.status === 'generating').length
    for (const [path, video] of wanted) {
      if (generating >= FILMSTRIP_CONCURRENCY) return
      if (path in current) continue
      startFilmstrip(video)
      generating += 1
    }
  }, [importedVideos, filmstrips])
}

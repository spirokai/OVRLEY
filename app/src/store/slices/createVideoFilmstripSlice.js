/**
 * Timeline filmstrips of the source videos, keyed by video path. Session state: not undoable, not saved.
 * A filmstrip is `generating` (by its job), `ready` with its sprite sheet, or `failed`.
 */

const FILMSTRIP_STATUSES = new Set(['generating', 'ready', 'failed'])

export const createVideoFilmstripSlice = (set) => ({
  videoFilmstrips: {},

  setVideoFilmstrip: (path, filmstrip) => {
    if (!FILMSTRIP_STATUSES.has(filmstrip.status)) throw new Error(`Unknown video filmstrip status: ${filmstrip.status}`)
    set((draft) => {
      draft.videoFilmstrips[path] = filmstrip
    })
  },

  removeVideoFilmstrip: (path) => {
    set((draft) => {
      delete draft.videoFilmstrips[path]
    })
  },
})

/**
 * Timeline filmstrips of the source videos. Session state: not undoable, not saved (strips live in the disk cache).
 *
 * `videoFilmstrips[path][key]` is one strip of a video, keyed by its window (`base`, or `z<level>-<window>` for a zoom
 * window): `generating` (by its job), `ready` with its sprite and sampling, or `failed`.
 * `wantedFilmstripWindows[path]` lists the zoom windows the timeline shows at its current zoom.
 */

const FILMSTRIP_STATUSES = new Set(['generating', 'ready', 'failed'])

export const createVideoFilmstripSlice = (set) => ({
  videoFilmstrips: {},
  wantedFilmstripWindows: {},

  setVideoFilmstrip: (path, key, strip) => {
    if (!FILMSTRIP_STATUSES.has(strip.status)) throw new Error(`Unknown video filmstrip status: ${strip.status}`)
    set((draft) => {
      draft.videoFilmstrips[path] ??= {}
      draft.videoFilmstrips[path][key] = strip
    })
  },

  removeVideoFilmstrip: (path, key) => {
    set((draft) => {
      const strips = draft.videoFilmstrips[path]
      if (!strips) return
      delete strips[key]
      if (Object.keys(strips).length === 0) delete draft.videoFilmstrips[path]
    })
  },

  removeVideoFilmstrips: (path) => {
    set((draft) => {
      delete draft.videoFilmstrips[path]
    })
  },

  /** Replaces the wanted zoom windows of every video: `{[path]: Array<{kind: 'zoom', level, window}>}`. */
  setWantedFilmstripWindows: (wanted) => {
    set((draft) => {
      draft.wantedFilmstripWindows = wanted
    })
  },
})

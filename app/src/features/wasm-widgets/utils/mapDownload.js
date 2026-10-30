import { prepareMapTiles } from '@/api/backend'

/** The map fields that decide which tiles a route needs. Colours and styles share the same tiles. */
const TILE_FIELDS = ['width', 'height', 'camera_mode', 'orientation', 'pitch', 'range_meters', 'show_full_activity']
/** A resize or a run of slider edits only downloads what it settles on. */
const SETTLE_MS = 500

/** @param {string} activityId Native snapshot identity. @param {object[]} maps Effective map widgets. @param {object} scene Effective scene. */
function request(activityId, maps, scene) {
  // Only a head start, since the preview fetches tiles on demand, so failures stay out of the session's errors.
  prepareMapTiles(activityId, maps, scene).catch((error) => console.warn(`Map route download: ${error?.message ?? error}`))
}

/**
 * Keeps the background route download on what the session's maps need.
 * @returns {{update: Function, dispose: Function}} Download of one preview session.
 */
export function createMapDownload() {
  let requested = ''
  let requestedScene = null
  let timer = null

  function stop() {
    clearTimeout(timer)
    if (requested !== '') request('', [], requestedScene)
    requested = ''
  }

  return {
    /** @param {object} snapshot Session snapshot with effective widgets, scene and activity. */
    update({ widgets, scene, activityId }) {
      const maps = widgets.filter((widget) => widget.type === 'map').map((widget) => widget.data)
      if (activityId === null || maps.length === 0) {
        stop()
        return
      }
      const wanted = JSON.stringify([activityId, scene, maps.map((map) => TILE_FIELDS.map((field) => map[field]))])
      if (wanted === requested) return
      clearTimeout(timer)
      requested = wanted
      requestedScene = scene
      timer = setTimeout(() => request(activityId, maps, scene), SETTLE_MS)
    },
    dispose: stop,
  }
}

/** Fetching and decoding use all of a core each, and the UI thread needs its own. */
const WORKER_COUNT = 2
/** A failed tile is external data, or the network. It's asked for again after this long. */
const RETRY_MS = 5000

/**
 * Decodes tiles off the UI thread. The latest view replaces the queue, since a tile a past camera
 * needed is no use, and tiles already being decoded still arrive and stay resident.
 * @param {object} options Renderer identity and callbacks.
 * @param {string} options.config Renderer config JSON.
 * @param {Uint8Array} options.style Style document.
 * @param {string} options.tileUrl Tile URL template.
 * @param {Function} options.onTile Receives `z, x, y` and the decoded geometry.
 * @param {Function} options.onFatal Receives the error when decoding can't work at all.
 * @returns {{want: Function, dispose: Function}} Worker pool of one renderer.
 */
export function createMapTileWorkers({ config, style, tileUrl, onTile, onFatal }) {
  const inFlight = new Set()
  const failed = new Map()
  let queue = []
  let idle = []

  function pump() {
    while (idle.length > 0 && queue.length > 0) {
      const worker = idle.pop()
      const [z, x, y] = queue.shift()
      inFlight.add(`${z}/${x}/${y}`)
      worker.postMessage({ z, x, y })
    }
  }

  const workers = Array.from({ length: WORKER_COUNT }, () => {
    const worker = new Worker(new URL('./mapTileWorker.js', import.meta.url), { type: 'module' })
    worker.onmessage = ({ data }) => {
      if (data.fatal) {
        onFatal(new Error(data.fatal))
        return
      }
      const key = `${data.z}/${data.x}/${data.y}`
      inFlight.delete(key)
      idle.push(worker)
      if (data.error) {
        console.warn(`Map tile ${key}: ${data.error}`)
        failed.set(key, performance.now())
      } else {
        failed.delete(key)
        onTile(data.z, data.x, data.y, data.tile)
      }
      pump()
    }
    worker.onerror = (event) => onFatal(new Error(event.message))
    worker.postMessage({ config, style, tileUrl })
    idle.push(worker)
    return worker
  })

  return {
    /** @param {Uint32Array} tiles `z, x, y` of each tile the view is missing. */
    want(tiles) {
      const now = performance.now()
      queue = []
      for (let at = 0; at < tiles.length; at += 3) {
        const key = `${tiles[at]}/${tiles[at + 1]}/${tiles[at + 2]}`
        if (inFlight.has(key)) continue
        const failedAt = failed.get(key)
        if (failedAt !== undefined && now - failedAt < RETRY_MS) continue
        queue.push([tiles[at], tiles[at + 1], tiles[at + 2]])
      }
      pump()
    },
    dispose() {
      queue = []
      idle = []
      for (const worker of workers) worker.terminate()
    },
  }
}

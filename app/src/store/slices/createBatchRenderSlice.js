/**
 * Batch render queue — renders many videos from one folder against the same
 * activity, template, and render settings, one at a time.
 */

function filenameFromPath(path) {
  return path.split(/[/\\]/).pop() || path
}

function buildQueueItem(path, previousItemsByPath) {
  const previous = previousItemsByPath.get(path)
  return {
    id: previous?.id ?? `${path}-${Math.random().toString(36).slice(2)}`,
    path,
    filename: filenameFromPath(path),
    skipOverlay: previous?.skipOverlay ?? false,
    status: 'pending',
    error: null,
  }
}

export function createBatchRenderSlice(set) {
  return {
    batchVideoFolder: null,
    batchOutputFolder: null,
    batchQueue: [],
    batchRunning: false,
    batchActiveItemId: null,

    setBatchVideoFolder: (path) => set({ batchVideoFolder: path || null }),

    setBatchOutputFolder: (path) => set({ batchOutputFolder: path || null }),

    setBatchQueueFromPaths: (paths) =>
      set((state) => {
        const previousItemsByPath = new Map(state.batchQueue.map((item) => [item.path, item]))
        state.batchQueue = (Array.isArray(paths) ? paths : []).map((path) => buildQueueItem(path, previousItemsByPath))
      }),

    removeBatchQueueItem: (id) =>
      set((state) => {
        state.batchQueue = state.batchQueue.filter((item) => item.id !== id)
      }),

    clearBatchQueue: () =>
      set({
        batchQueue: [],
        batchVideoFolder: null,
        batchActiveItemId: null,
      }),

    setBatchItemSkipOverlay: (id, skipOverlay) =>
      set((state) => {
        const item = state.batchQueue.find((candidate) => candidate.id === id)
        if (item) item.skipOverlay = Boolean(skipOverlay)
      }),

    setBatchItemStatus: (id, status, error = null) =>
      set((state) => {
        const item = state.batchQueue.find((candidate) => candidate.id === id)
        if (item) {
          item.status = status
          item.error = error
        }
      }),

    setBatchRunning: (running) => set({ batchRunning: Boolean(running) }),

    setBatchActiveItemId: (id) => set({ batchActiveItemId: id }),

    resetBatchQueueStatuses: () =>
      set((state) => {
        for (const item of state.batchQueue) {
          if (item.status === 'blocked' || item.status === 'checking') continue
          item.status = 'pending'
          item.error = null
        }
      }),
  }
}

const PREVIEW_PERF_FLAG = 'ovrley:preview-perf'

const counterState = {
  counts: Object.create(null),
  values: Object.create(null),
  timerId: null,
}

function isPreviewPerfEnabled() {
  if (!import.meta.env.DEV || typeof window === 'undefined') {
    return false
  }

  try {
    return window.localStorage.getItem(PREVIEW_PERF_FLAG) === 'true'
  } catch {
    return false
  }
}

function flushPreviewPerfCounters() {
  const snapshot = {
    ...counterState.values,
    ...counterState.counts,
  }
  counterState.counts = Object.create(null)

  if (!Object.keys(snapshot).length) {
    return
  }

  window.__OVRLEY_PREVIEW_PERF__ = snapshot
  console.info('[preview-perf]', snapshot)
}

function ensurePreviewPerfTimer() {
  if (!isPreviewPerfEnabled()) {
    if (counterState.timerId !== null) {
      window.clearInterval(counterState.timerId)
      counterState.timerId = null
      counterState.counts = Object.create(null)
      counterState.values = Object.create(null)
    }

    return false
  }

  if (counterState.timerId === null) {
    counterState.timerId = window.setInterval(flushPreviewPerfCounters, 1000)
  }

  return true
}

export function incrementPreviewPerfCounter(counterName, amount = 1) {
  if (!ensurePreviewPerfTimer()) {
    return
  }

  const safeAmount = Number.isFinite(amount) ? amount : 1
  counterState.counts[counterName] =
    (counterState.counts[counterName] || 0) + safeAmount
}

export function previewPerfCounterName(label) {
  return `${label}/s`
}

export function setPreviewPerfValue(label, value) {
  if (!ensurePreviewPerfTimer()) {
    return
  }

  counterState.values[label] = value
}

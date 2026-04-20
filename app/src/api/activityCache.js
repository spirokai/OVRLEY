let currentParsedActivity = null
let currentParsedActivityDebug = null

export function setCurrentActivityCache(activity, debugPayload = null) {
  currentParsedActivity = activity
  currentParsedActivityDebug = debugPayload
}

export function getCurrentParsedActivity() {
  return currentParsedActivity
}

export function getCurrentParsedActivityDebug() {
  return currentParsedActivityDebug
}

export function clearCurrentActivityCache() {
  currentParsedActivity = null
  currentParsedActivityDebug = null
}

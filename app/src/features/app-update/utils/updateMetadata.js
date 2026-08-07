/**
 * Validates updater metadata at the plugin boundary and returns the frontend shape.
 *
 * @param {*} update - Metadata returned by the Tauri updater plugin.
 * @returns {{version: string, downloadAndInstall: function}} Canonical update metadata.
 */
export function readUpdateMetadata(update) {
  if (!update || typeof update !== 'object') {
    throw new Error('Updater returned malformed update metadata')
  }
  if (typeof update.version !== 'string' || !update.version.trim()) {
    throw new Error('Updater returned an invalid update version')
  }
  if (typeof update.downloadAndInstall !== 'function') {
    throw new Error('Updater returned an update without download support')
  }
  return {
    version: update.version,
    downloadAndInstall: Function.prototype.bind.call(update.downloadAndInstall, update),
  }
}

/**
 * Validates one download event at the updater plugin boundary.
 *
 * @param {*} event - Event returned by `downloadAndInstall`.
 * @returns {object} Canonical download event.
 */
export function readDownloadEvent(event) {
  if (!event || typeof event !== 'object' || typeof event.event !== 'string') {
    throw new Error('Updater returned malformed download progress')
  }

  if (event.event === 'Started') {
    if (!event.data || typeof event.data !== 'object') {
      throw new Error('Updater returned malformed download start data')
    }
    const contentLength = event.data.contentLength
    if (
      contentLength !== undefined &&
      contentLength !== null &&
      (typeof contentLength !== 'number' || !Number.isFinite(contentLength) || contentLength < 0)
    ) {
      throw new Error('Updater returned an invalid download size')
    }
    return { type: 'started', contentLength: contentLength ?? null }
  }

  if (event.event === 'Progress') {
    if (!event.data || typeof event.data !== 'object') {
      throw new Error('Updater returned malformed download progress data')
    }
    const chunkLength = event.data.chunkLength
    if (typeof chunkLength !== 'number' || !Number.isInteger(chunkLength) || chunkLength < 0) {
      throw new Error('Updater returned an invalid download chunk size')
    }
    return { type: 'progress', chunkLength }
  }

  if (event.event === 'Finished') {
    return { type: 'finished' }
  }

  throw new Error(`Unknown updater download event: ${String(event.event)}`)
}

import { load } from '@tauri-apps/plugin-store'
import { isAbsolute } from '@tauri-apps/api/path'

let _store = null

async function getStore() {
  if (!_store) {
    _store = await load('ovrley-settings.json', { autoSave: true })
  }
  return _store
}

export async function getPreference(key) {
  const store = await getStore()
  return await store.get(key)
}

/**
 * Reads an optional absolute-path preference through one strict contract.
 * An unavailable preference store is treated as documented optional absence.
 * @param {string} key Preference key.
 * @returns {Promise<string|null>} Stored absolute path, or null when absent.
 */
export async function getOptionalPathPreference(key) {
  let value
  try {
    value = await getPreference(key)
  } catch {
    return null
  }

  if (value === null || value === undefined) return null
  if (typeof value !== 'string' || !value.trim() || value !== value.trim() || !(await isAbsolute(value))) {
    throw new Error(`Preference "${key}" must be an absolute path`)
  }
  return value
}

export async function setPreference(key, value) {
  const store = await getStore()
  await store.set(key, value)
}

export async function deletePreference(key) {
  const store = await getStore()
  await store.delete(key)
}

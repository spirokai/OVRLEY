import { load } from '@tauri-apps/plugin-store'

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

export async function setPreference(key, value) {
  const store = await getStore()
  await store.set(key, value)
}

export async function deletePreference(key) {
  const store = await getStore()
  await store.delete(key)
}

import { open } from '@tauri-apps/plugin-dialog'
import { readSelectedFileBytes } from '@/api/backend'
import { getPreference, setPreference } from '@/lib/preferences-store'

export const selectBrowserFile = (accept) =>
  new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = accept
    input.onchange = () => resolve(input.files?.[0] ?? null)
    input.oncancel = () => resolve(null)
    input.click()
  })

export async function fileFromSelectedPath(selectedPath, fallbackName = 'file') {
  const bytes = await readSelectedFileBytes(selectedPath)
  const filename = String(selectedPath).split(/[/\\]/).pop() || fallbackName
  return new File([bytes], filename, { type: 'application/octet-stream' })
}

export async function openSinglePath(filters, options = {}) {
  const { lastDirectoryKey } = options
  let defaultPath

  if (lastDirectoryKey) {
    try {
      const saved = await getPreference(lastDirectoryKey)
      if (saved) defaultPath = saved
    } catch {
      // store unavailable — proceed without default path
    }
  }

  const selected = await open({
    multiple: false,
    filters,
    ...(defaultPath ? { defaultPath } : {}),
  })

  if (selected && lastDirectoryKey) {
    try {
      const dir = selected.replace(/[\\/][^\\/]*$/, '')
      await setPreference(lastDirectoryKey, dir)
    } catch {
      // store may be unavailable
    }
  }

  return selected
}

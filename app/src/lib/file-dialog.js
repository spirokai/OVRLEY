import { convertFileSrc } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

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
  const response = await fetch(convertFileSrc(selectedPath))
  if (!response.ok) {
    throw new Error(`Failed to read selected file: ${response.status}`)
  }

  const blob = await response.blob()
  const filename = String(selectedPath).split(/[/\\]/).pop() || fallbackName
  return new File([blob], filename, { type: blob.type || 'application/octet-stream' })
}

export function openSinglePath(filters) {
  return open({
    multiple: false,
    filters,
  })
}

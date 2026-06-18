import { open } from '@tauri-apps/plugin-dialog'
import { readSelectedFileBytes } from '@/api/backend'

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

export function openSinglePath(filters) {
  return open({
    multiple: false,
    filters,
  })
}

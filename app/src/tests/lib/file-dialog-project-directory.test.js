import { beforeEach, expect, test, vi } from 'vitest'

const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }))
const preferences = vi.hoisted(() => ({ getPreference: vi.fn(), setPreference: vi.fn() }))

vi.mock('@tauri-apps/plugin-dialog', () => dialog)
vi.mock('@/api/backend', () => ({ readSelectedFileBytes: vi.fn() }))
vi.mock('@/lib/preferences-store', () => preferences)

import { openSinglePath, saveSinglePath } from '@/lib/file-dialog'

beforeEach(() => {
  vi.clearAllMocks()
})

test('project open and Save As share one persistent directory preference', async () => {
  let rememberedDirectory = 'D:\\OVRLEY Projects'
  preferences.getPreference.mockImplementation(async () => rememberedDirectory)
  preferences.setPreference.mockImplementation(async (_key, directory) => {
    rememberedDirectory = directory
  })
  dialog.open.mockResolvedValue('E:\\Events\\Race.oly')
  dialog.save.mockResolvedValue('F:\\Archive\\Race.oly')

  await openSinglePath([{ name: 'OVRLEY Project', extensions: ['oly'] }], {
    defaultPath: 'C:\\Users\\test\\Documents\\OVRLEY\\projects',
    lastDirectoryKey: 'last-project-dir',
  })
  await saveSinglePath('C:\\Users\\test\\Documents\\OVRLEY\\projects\\Race.oly', 'oly', 'OVRLEY Project', {
    lastDirectoryKey: 'last-project-dir',
  })

  expect(dialog.open).toHaveBeenCalledWith({
    multiple: false,
    filters: [{ name: 'OVRLEY Project', extensions: ['oly'] }],
    defaultPath: 'D:\\OVRLEY Projects',
  })
  expect(dialog.save).toHaveBeenCalledWith({
    defaultPath: 'E:\\Events\\Race.oly',
    filters: [{ name: 'OVRLEY Project', extensions: ['oly'] }],
  })
  expect(preferences.setPreference).toHaveBeenNthCalledWith(1, 'last-project-dir', 'E:\\Events')
  expect(preferences.setPreference).toHaveBeenNthCalledWith(2, 'last-project-dir', 'F:\\Archive')
})

test('cancelled project dialogs do not change the remembered directory', async () => {
  preferences.getPreference.mockResolvedValue('D:\\OVRLEY Projects')
  dialog.open.mockResolvedValue(null)
  dialog.save.mockResolvedValue(null)

  await openSinglePath([{ name: 'OVRLEY Project', extensions: ['oly'] }], { lastDirectoryKey: 'last-project-dir' })
  await saveSinglePath('D:\\OVRLEY Projects\\Race.oly', 'oly', 'OVRLEY Project', { lastDirectoryKey: 'last-project-dir' })

  expect(preferences.setPreference).not.toHaveBeenCalled()
})

import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, expect, test, vi } from 'vitest'
import * as backend from '@/api/backend'
import { loadProject } from '@/features/projects/projectOperations'
import { createProjectSnapshot } from '@/features/projects/utils/projectSnapshot'
import useProjectSourceRecovery from '@/features/projects/hooks/useProjectSourceRecovery'
import useStore from '@/store/useStore'

vi.mock('@/lib/font-resources', () => ({ prepareDocumentFonts: vi.fn(async () => {}) }))
vi.mock('@/api/backend', () => ({
  readProjectFile: vi.fn(),
  prepareVideoSequence: vi.fn(),
  registerPreviewSequence: vi.fn(),
  extractVideoSequenceTelemetry: vi.fn(async () => null),
  selectedPathIsFile: vi.fn(),
  detectCodecs: vi.fn(),
}))

beforeEach(() => {
  vi.clearAllMocks()
  useStore.setState(useStore.getInitialState(), true)
})

test('reopens originals with a fresh manifest and restores the common offset', async () => {
  const parts = [
    { path: 'C:/one.mp4', duration: 10, startSeconds: 0 },
    { path: 'C:/two.mp4', duration: 20, startSeconds: 10 },
  ]
  useStore.setState({ importedVideoPath: 'C:/old.ffconcat', importedVideoParts: parts, videoSyncOffsetSeconds: 4 })
  const project = createProjectSnapshot(useStore.getState(), 'C:/Ride.oly')
  useStore.setState(useStore.getInitialState(), true)
  const sources = { activityPath: null, videoPath: 'C:/one.mp4', videoFollowingPaths: ['C:/two.mp4'] }
  backend.readProjectFile.mockResolvedValue({ project, resolvedSources: sources, rasterLoadResults: {} })
  backend.prepareVideoSequence.mockResolvedValue({
    metadata: { path: 'C:/new.ffconcat', duration: 30, fps: 25, resolution: { width: 1920, height: 1080 }, parts },
    warnings: [],
  })
  backend.registerPreviewSequence.mockResolvedValue([
    { importId: 'one', previewUrl: 'http://localhost/one' },
    { importId: 'two', previewUrl: 'http://localhost/two' },
  ])
  await loadProject({ path: 'C:/Ride.oly', resolveProjectSources: async (resolved) => resolved })
  expect(backend.prepareVideoSequence).toHaveBeenCalledWith(['C:/one.mp4', 'C:/two.mp4'])
  expect(backend.registerPreviewSequence).toHaveBeenCalledWith(['C:/one.mp4', 'C:/two.mp4'])
  expect(useStore.getState().importedVideoPath).toBe('C:/new.ffconcat')
  expect(useStore.getState().importedVideoParts[1].previewUrl).toBe('http://localhost/two')
  expect(useStore.getState().videoSyncOffsetSeconds).toBe(4)
})

test('skipping a missing chapter skips the entire group instead of shortening its timeline', async () => {
  backend.selectedPathIsFile.mockImplementation(async (path) => path !== 'C:/two.mp4')
  const { result } = renderHook(() => useProjectSourceRecovery())
  let pending
  act(() => {
    pending = result.current.resolveProjectSources({ activityPath: null, videoPath: 'C:/one.mp4', videoFollowingPaths: ['C:/two.mp4'] })
  })
  await waitFor(() => expect(result.current.dialog.open).toBe(true))
  let resolved
  await act(async () => {
    result.current.dialog.onLoadAnyway()
    resolved = await pending
  })
  expect(resolved).toEqual({ activityPath: null, videoPath: null })
})

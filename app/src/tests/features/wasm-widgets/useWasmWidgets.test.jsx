import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vite-plus/test'
import { useWasmWidgets } from '@/features/wasm-widgets/hooks/useWasmWidgets'
import { createPreviewSession, loadWidgetModule } from '@/features/wasm-widgets/utils/previewSession'
import useStore from '@/store/useStore'

vi.mock('@/features/wasm-widgets/utils/previewSession', () => ({ createPreviewSession: vi.fn(), loadWidgetModule: vi.fn() }))

afterEach(() => {
  useStore.setState(useStore.getInitialState(), true)
})

test('a replaced editor document gets a fresh preview session with its canvases', async () => {
  const sessions = []
  loadWidgetModule.mockResolvedValue({})
  createPreviewSession.mockImplementation(() => {
    const session = { setSnapshot: vi.fn(), attach: vi.fn(), attachMapLayer: vi.fn(), dispose: vi.fn() }
    sessions.push(session)
    return session
  })
  const props = { widgets: [], scene: {}, activity: null, disabledLaps: [], defaultFont: 'Inter.ttf' }
  const { result } = renderHook(() => useWasmWidgets(props))
  await waitFor(() => expect(sessions).toHaveLength(1))
  const canvas = document.createElement('canvas')
  act(() => result.current.attach('widget-1', canvas, canvas))
  const before = result.current.documentRevision

  act(() => useStore.setState((state) => ({ editorDocumentRevision: state.editorDocumentRevision + 1 })))

  expect(result.current.documentRevision).toBe(before + 1)
  expect(sessions[0].dispose).toHaveBeenCalledOnce()
  await waitFor(() => expect(sessions).toHaveLength(2))
  expect(sessions[1].attach).toHaveBeenCalledWith('widget-1', canvas, canvas)
  expect(sessions[1].setSnapshot).toHaveBeenCalled()
})

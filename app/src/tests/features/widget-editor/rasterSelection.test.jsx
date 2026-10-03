import { act, render, renderHook, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import { useRasterSelection } from '@/features/widget-editor/hooks/useRasterSelection'
import RasterPreview from '@/features/widget-preview/widgets/raster/RasterPreview'
import { resolveWidgetRenderGeometry } from '@/features/overlay-editor/utils/widgetRenderGeometry'
import { constrainRasterResize } from '@/features/overlay-editor/utils/widgetResizeScaling'
import { redoHistory, undoHistory } from '@/features/undo-redo/undoHistory'
import { deleteWidgetsInConfig, duplicateWidgetsInConfig } from '@/lib/widget/widget-config'
import useStore from '@/store/useStore'
import { cloneSerializable, DEFAULT_CONFIG } from '@/store/store-utils'
import { loadSelectedRaster, rasterPreviewPng } from '@/api/backend'
import { openSinglePath } from '@/lib/file-dialog'

vi.mock('@/api/backend', () => ({ loadSelectedRaster: vi.fn(), rasterPreviewPng: vi.fn() }))
vi.mock('@/lib/file-dialog', () => ({ openSinglePath: vi.fn() }))

const liveEditsSnapshot = { activeWidgetInteraction: null, liveWidgetDrafts: {} }
const widgetLiveEdits = {
  getSnapshot: () => liveEditsSnapshot,
  subscribe: () => () => {},
}

describe('raster creation and selection', () => {
  beforeEach(() => {
    useStore.getState().setConfig(cloneSerializable(DEFAULT_CONFIG))
    useStore.temporal.getState().clear()
    vi.resetAllMocks()
  })

  test('creates and selects an exact canonical placeholder without opening a picker', () => {
    const { result } = renderHook(() => useWidgetManager({ widgetLiveEdits }))
    act(() => result.current.addWidget({ type: 'raster' }))

    const raster = useStore.getState().config.rasters[0]
    expect(Object.keys(raster).sort()).toEqual(['height', 'id', 'opacity', 'path', 'rotation', 'width', 'x', 'y'])
    expect(raster.path).toBeNull()
    expect(useStore.getState().selectedWidgetId).toBe(raster.id)
    expect(openSinglePath).not.toHaveBeenCalled()
    render(
      <div style={{ width: raster.width, height: raster.height }}>
        <RasterPreview widget={{ data: raster }} globalOpacity={1} />
      </div>,
    )
    expect(screen.getByTestId('raster-placeholder')).toHaveStyle({ opacity: '0.5' })
  })

  test('selects Rust-owned image bytes at exact oriented dimensions', async () => {
    const { result: manager } = renderHook(() => useWidgetManager({ widgetLiveEdits }))
    act(() => manager.current.addWidget({ type: 'raster' }))
    const id = useStore.getState().config.rasters[0].id
    vi.mocked(openSinglePath).mockResolvedValue('C:\\images\\ride.tiff')
    vi.mocked(loadSelectedRaster).mockResolvedValue({
      width: 480,
      height: 640,
      resourceId: 'original',
    })
    vi.mocked(rasterPreviewPng).mockResolvedValue('aGVsbG8=')

    const { result } = renderHook(() => useRasterSelection(id))
    await act(async () => result.current.selectImage())

    expect(openSinglePath).toHaveBeenCalledWith([{ name: expect.any(String), extensions: ['png', 'jpeg', 'jpg', 'bmp', 'tiff'] }])
    expect(useStore.getState().config.rasters[0]).toMatchObject({ width: 480, height: 640, path: 'C:\\images\\ride.tiff' })
    expect(useStore.getState().config.rasters[0]).toHaveProperty('resourceId', 'original')
    expect(resolveWidgetRenderGeometry({ type: 'raster', category: 'rasters', data: useStore.getState().config.rasters[0] }, null, 2)).toMatchObject({
      width: 960,
      height: 1280,
    })
    const { container } = render(<RasterPreview widget={{ id, data: useStore.getState().config.rasters[0] }} globalOpacity={1} />)
    await waitFor(() => expect(container.querySelector('img')).toHaveAttribute('src', 'data:image/png;base64,aGVsbG8='))
    expect(rasterPreviewPng).toHaveBeenCalledWith('original')
  })

  test('leaves an existing image and dimensions intact on failed replacement', async () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.rasters.push({ id: 'widget-1', x: 100, y: 100, width: 200, height: 100, rotation: 0, opacity: 1, path: 'C:\\images\\old.png' })
    useStore.getState().setConfig(config)
    useStore.getState().setRasterImage('widget-1', 'C:\\images\\old.png', {
      width: 200,
      height: 100,
      resourceId: 'old-image',
    })
    const before = useStore.getState().config.rasters[0]
    const error = new Error('The image exceeds the decoded resolution limit.')
    error.code = 'resolution'
    vi.mocked(openSinglePath).mockResolvedValue('C:\\images\\huge.png')
    vi.mocked(loadSelectedRaster).mockRejectedValue(error)

    const { result } = renderHook(() => useRasterSelection('widget-1'))
    await act(async () => result.current.selectImage())

    expect(useStore.getState().config.rasters[0]).toEqual(before)
    expect(useStore.getState().config.rasters[0].resourceId).toBe('old-image')
    expect(result.current.error).toMatch(/25/)
  })

  test('shows a recovered template resource error and clears it after replacement', async () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.rasters.push({
      id: 'widget-1',
      x: 100,
      y: 100,
      width: 200,
      height: 100,
      rotation: 0,
      opacity: 1,
      path: 'C:\\images\\missing.png',
      resourceErrorCode: 'missing',
    })
    useStore.getState().setConfig(config)
    vi.mocked(openSinglePath).mockResolvedValue('C:\\images\\replacement.png')
    vi.mocked(loadSelectedRaster).mockResolvedValue({
      width: 640,
      height: 360,
      resourceId: 'replacement-resource',
    })

    const { result } = renderHook(() => useRasterSelection('widget-1'))
    expect(result.current.error).toMatch(/missing/i)

    await act(async () => result.current.selectImage())

    expect(result.current.error).toBeNull()
    expect(useStore.getState().config.rasters[0]).toMatchObject({
      path: 'C:\\images\\replacement.png',
      resourceId: 'replacement-resource',
    })
    expect(useStore.getState().config.rasters[0]).not.toHaveProperty('resourceErrorCode')
  })

  test('undo and redo restore bytes and dimensions across same-path replacement', () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.rasters.push({ id: 'raster-1', x: 0, y: 0, width: 100, height: 100, rotation: 0, opacity: 1, path: null })
    useStore.getState().setConfig(config)
    useStore.temporal.getState().clear()
    const path = 'C:\\images\\ride.png'
    useStore.getState().setRasterImage('raster-1', path, { width: 200, height: 100, resourceId: 'image-first' })
    useStore.getState().setLastRenderedConfig(useStore.getState().config)
    useStore.getState().setRasterImage('raster-1', path, { width: 300, height: 150, resourceId: 'image-second' })
    expect(useStore.getState().hasUnrenderedChanges).toBe(true)
    undoHistory(useStore)
    expect(useStore.getState().config.rasters[0]).toMatchObject({ width: 200, height: 100 })
    expect(useStore.getState().config.rasters[0].resourceId).toBe('image-first')
    expect(useStore.getState().hasUnrenderedChanges).toBe(false)
    redoHistory(useStore)
    expect(useStore.getState().config.rasters[0]).toMatchObject({ width: 300, height: 150 })
    expect(useStore.getState().config.rasters[0].resourceId).toBe('image-second')
  })

  test('a duplicated raster owns an independent resource', () => {
    const config = cloneSerializable(DEFAULT_CONFIG)
    config.rasters.push({ id: 'source', x: 0, y: 0, width: 100, height: 100, rotation: 0, opacity: 1, path: null })
    useStore.getState().setConfig(config)
    useStore.getState().setRasterImage('source', 'C:\\images\\first.png', { width: 100, height: 100, resourceId: 'image-first' })
    const source = useStore.getState().config.rasters[0]
    const { config: duplicated, insertedWidgetIds } = duplicateWidgetsInConfig(useStore.getState().config, [{ category: 'rasters', data: source }])
    const duplicateId = insertedWidgetIds[0]
    useStore.getState().setConfig(duplicated)
    useStore.getState().setRasterImage(duplicateId, 'C:\\images\\second.png', { width: 50, height: 80, resourceId: 'image-second' })
    expect(useStore.getState().config.rasters.find((raster) => raster.id === 'source').resourceId).toBe('image-first')
    expect(useStore.getState().config.rasters.find((raster) => raster.id === duplicateId).resourceId).toBe('image-second')
    useStore.getState().setConfig(deleteWidgetsInConfig(useStore.getState().config, [duplicateId]))
    expect(useStore.getState().config.rasters.find((raster) => raster.id === duplicateId)).toBeUndefined()
    undoHistory(useStore)
    expect(useStore.getState().config.rasters.find((raster) => raster.id === duplicateId).resourceId).toBe('image-second')
  })

  test('corner resizing preserves displayed ratio while edges stretch independently', () => {
    const origin = { x: 20, y: 30, width: 200, height: 100, direction: [-1, -1] }
    expect(constrainRasterResize(origin, { x: 0, y: 10, width: 300, height: 120 }, 1)).toEqual({ x: -80, y: -20, width: 300, height: 150 })
    expect(constrainRasterResize(origin, { x: 0, y: 10, width: 300, height: 120 }, 2)).toEqual({ x: -180, y: -70, width: 300, height: 150 })
    expect(constrainRasterResize({ ...origin, direction: [1, 0] }, { x: 20, y: 30, width: 300, height: 100 }, 1)).toEqual({
      x: 20,
      y: 30,
      width: 300,
      height: 100,
    })
  })
})

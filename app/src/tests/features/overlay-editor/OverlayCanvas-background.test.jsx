import { render } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import OverlayCanvas from '@/features/overlay-editor/components/OverlayCanvas'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'

vi.mock('@/features/video-preview', () => ({ VideoPreviewSurface: () => <div data-testid="video-preview-surface" /> }))
vi.mock('@/features/wasm-widgets/hooks/useWasmWidgets', () => ({ useWasmWidgets: () => ({ attach: vi.fn(), error: null }) }))

const state = {
  globalDefaults: DEFAULT_GLOBAL_DEFAULTS,
  sceneSize: { width: 1920, height: 1080 },
  scene: {},
  widgets: [],
  activity: null,
  previewSecond: 0,
  setSceneElement: vi.fn(),
  widgetRefCallbacks: {},
  backgroundImageSrc: null,
  gridRef: { current: null },
  gridStyle: { transform: 'scale(1)', transformOrigin: 'top left' },
}

describe('OverlayCanvas background modes', () => {
  test.each([
    ['checker', 'rgb(0, 0, 0)'],
    ['black', 'rgb(0, 0, 0)'],
    ['white', 'rgb(244, 234, 210)'],
  ])('%s mode renders its configured background', (backgroundMode, color) => {
    const { getByTestId } = render(
      <OverlayCanvas state={state} backgroundMode={backgroundMode} displayScale={1} gridVisible={false} handleWidgetMouseDown={vi.fn()} />,
    )
    const background = getByTestId('overlay-scene').firstElementChild
    expect(background.style.backgroundColor).toBe(color)
    expect(background.classList.contains('bg-overlay-grid-muted')).toBe(backgroundMode === 'checker')
  })

  test('transparent mode omits the background layer', () => {
    const { getByTestId } = render(
      <OverlayCanvas state={state} backgroundMode="transparent" displayScale={1} gridVisible={false} handleWidgetMouseDown={vi.fn()} />,
    )
    expect(getByTestId('overlay-scene').firstElementChild).toBe(getByTestId('widget-layer'))
  })

  test('video mode mounts the video surface', () => {
    const { getByTestId } = render(
      <OverlayCanvas state={state} backgroundMode="video" displayScale={1} gridVisible={false} handleWidgetMouseDown={vi.fn()} />,
    )
    expect(getByTestId('video-preview-surface')).toBeTruthy()
  })
})

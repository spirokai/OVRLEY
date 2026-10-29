import { render } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import OverlayMoveable from '@/features/overlay-editor/components/OverlayMoveable'

const moveable = vi.hoisted(() => vi.fn(() => null))
vi.mock('react-moveable', () => ({ default: moveable }))

const props = {
  canResizeSelected: true,
  canRotateSelected: false,
  elementGuidelines: [],
  horizontalGuidelines: [0, 540, 1080],
  verticalGuidelines: [0, 960, 1920],
  zoom: 0.75,
  handlers: { onResize: vi.fn(), onDragGroup: vi.fn(), onResizeGroupStart: vi.fn(), onResizeGroup: vi.fn(), onResizeGroupEnd: vi.fn() },
  isGroupDragActive: false,
  isGroupSelection: false,
  maintainAspectRatio: true,
  moveableRef: { current: null },
  sceneElement: document.createElement('div'),
  selectedTarget: document.createElement('div'),
  selectedTargets: [],
  showEdgeResizeHandles: false,
}

describe('OverlayMoveable WASM gestures', () => {
  beforeEach(() => moveable.mockClear())

  test('uses resize handlers and viewport guides without legacy scaling', () => {
    render(<OverlayMoveable {...props} />)
    expect(moveable.mock.lastCall[0]).toMatchObject({
      resizable: true,
      scalable: false,
      checkResizableError: false,
      keepRatio: true,
      renderDirections: ['nw', 'ne', 'se', 'sw'],
      horizontalGuidelines: props.horizontalGuidelines,
      verticalGuidelines: props.verticalGuidelines,
      onResize: props.handlers.onResize,
    })
    expect(moveable.mock.lastCall[0].onScale).toBeUndefined()
  })

  test('enables proportional group resize through the bounding-box handles', () => {
    const selectedTargets = [document.createElement('div'), document.createElement('div')]
    render(<OverlayMoveable {...props} selectedTarget={null} selectedTargets={selectedTargets} isGroupSelection isGroupDragActive />)
    expect(moveable.mock.lastCall[0]).toMatchObject({
      targets: selectedTargets,
      groupable: true,
      hideChildMoveableDefaultLines: true,
      resizable: true,
      scalable: false,
      keepRatio: true,
      renderDirections: ['nw', 'ne', 'se', 'sw'],
      onDragGroup: props.handlers.onDragGroup,
      onResizeGroupStart: props.handlers.onResizeGroupStart,
      onResizeGroup: props.handlers.onResizeGroup,
      onResizeGroupEnd: props.handlers.onResizeGroupEnd,
    })
  })
})

import { render } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
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
  handlers: { onResize: vi.fn(), onDragGroup: vi.fn() },
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

  test('keeps group translation enabled and group scaling disabled', () => {
    const selectedTargets = [document.createElement('div'), document.createElement('div')]
    render(
      <OverlayMoveable
        {...props}
        selectedTarget={null}
        selectedTargets={selectedTargets}
        isGroupSelection
        isGroupDragActive
        canResizeSelected={false}
      />,
    )
    expect(moveable.mock.lastCall[0]).toMatchObject({
      targets: selectedTargets,
      groupable: true,
      hideChildMoveableDefaultLines: true,
      resizable: false,
      scalable: false,
      onDragGroup: props.handlers.onDragGroup,
    })
  })
})

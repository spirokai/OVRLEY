import { act, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import OverlayPlayer from '@/features/player/components/OverlayPlayer'
import useStore from '@/store/useStore'

function installResizeObserver() {
  globalThis.ResizeObserver = class ResizeObserver {
    constructor(callback) {
      this.callback = callback
    }

    observe() {
      this.callback([{ contentRect: { width: 500 } }])
    }

    disconnect() {}
  }
}

function clearResizeObserver() {
  delete globalThis.ResizeObserver
}

function resetStore(overrides = {}) {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState({
    activitySource: { kind: 'file', path: 'C:\\activities\\ride.fit' },
    activitySummary: {
      availableMetrics: [],
      durationSeconds: 100,
      fileFormat: 'fit',
      fileName: 'activity.fit',
    },
    fallbackDurationSeconds: 73,
    ...overrides,
  })
}

describe('OverlayPlayer', () => {
  const playerProps = {
    activeKeyboardWorkspace: 'player',
    backgroundMode: 'black',
    onActivateKeyboardWorkspace: vi.fn(),
  }

  beforeEach(() => {
    vi.restoreAllMocks()
    installResizeObserver()
    resetStore()
  })

  test('renders the presentational toolbar and timeline through the public component', () => {
    render(<OverlayPlayer {...playerProps} />)

    expect(screen.getByRole('group', { name: 'Timeline' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Play' })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Activity' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByLabelText('ride.fit')).toBeInTheDocument()
  })

  test('renders nothing when there is no activity and no imported video', () => {
    resetStore({
      activitySource: null,
      activitySummary: null,
      importedVideoPath: null,
    })

    render(<OverlayPlayer {...playerProps} />)

    expect(screen.queryByRole('group', { name: 'Timeline' })).not.toBeInTheDocument()
  })

  test('measures and renders clip geometry when timeline mounts after initially hidden state', () => {
    clearResizeObserver()
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      bottom: 0,
      height: 0,
      left: 0,
      right: 500,
      top: 0,
      width: 500,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    })
    resetStore({
      activitySource: null,
      activitySummary: null,
      importedVideoPath: null,
    })

    const { rerender } = render(<OverlayPlayer {...playerProps} />)

    expect(screen.queryByRole('group', { name: 'Timeline' })).not.toBeInTheDocument()

    act(() => resetStore())
    rerender(<OverlayPlayer {...playerProps} />)

    const clip = screen.getByLabelText('ride.fit')
    expect(clip).toBeInTheDocument()
    expect(clip).not.toHaveStyle({ width: '0%' })
  })
})

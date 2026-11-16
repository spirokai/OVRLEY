import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vite-plus/test'
import { Video } from 'lucide-react'
import TimelineLane from '@/features/player/components/TimelineLane'

function createClip(overrides = {}) {
  return {
    clipClassName: 'bg-accent/70',
    clipContentClassName: 'translate-y-[0.04rem]',
    clipProps: {},
    clipStyle: { left: '0%', width: '50%' },
    segments: [],
    durationLabel: '00:20',
    formatLabel: 'FIT',
    trimMasks: [],
    textStyle: { left: '0%', width: '100%' },
    icon: null,
    id: 'activity',
    isVideo: false,
    isVisible: true,
    label: 'ride.fit',
    showText: true,
    sourceColumnWidth: '1.5rem',
    textClassName: 'text-accent-foreground',
    tooltip: {
      id: 'tooltip-activity',
      isVisible: false,
      style: { left: '50%' },
    },
    warnings: [],
    ...overrides,
  }
}

function createLane(clipOverrides = {}, laneOverrides = {}) {
  return {
    ariaLabel: 'Activity clip lane',
    id: 'activity',
    isVideo: false,
    clips: [createClip(clipOverrides)],
    conflictBands: [],
    cutMarkers: [],
    joinMarkers: [],
    skippedJoinMarkers: [],
    ...laneOverrides,
  }
}

function renderLane(overrides = {}) {
  return render(<TimelineLane lane={createLane(overrides)} />)
}

function createVideoSegment(overrides = {}) {
  return {
    key: 'start',
    excluded: false,
    missing: false,
    hasSeamBefore: false,
    hasSeamAfter: false,
    isSelected: false,
    style: { left: '0%', width: '100%' },
    tiles: [],
    ...overrides,
  }
}

function renderVideoLane(clipOverrides = {}) {
  const clip = {
    clipClassName: '',
    icon: Video,
    id: 'video:video-1',
    isVideo: true,
    label: 'ride.mp4',
    formatLabel: 'MP4',
    badgeStyle: { left: '25px', maxWidth: '75px' },
    segments: [createVideoSegment()],
    tooltip: { id: 'tooltip-video', isVisible: false, style: { left: '50%' } },
    ...clipOverrides,
  }
  return render(<TimelineLane lane={createLane(clip, { ariaLabel: 'Video clip lane', id: 'video', isVideo: true })} />)
}

describe('TimelineLane', () => {
  test("shows a video clip's name and duration in a badge at its visible left edge", () => {
    renderVideoLane()

    const name = screen.getByText('ride.mp4')
    expect(screen.getByText('00:20')).toBeInTheDocument()
    expect(name.closest('[style]')).toHaveStyle({ left: '25px', maxWidth: '75px' })
    // The badge replaces the row of the activity clip.
    expect(screen.queryByText('MP4')).not.toBeInTheDocument()
  })

  test('draws the filmstrip tiles of a video segment', () => {
    renderVideoLane({
      segments: [
        createVideoSegment({
          tiles: [
            { key: 0, style: { left: '0px', width: '85px', backgroundImage: 'url("blob:ride")', backgroundPosition: '0% 0%' } },
            { key: 1, style: { left: '85px', width: '85px', backgroundImage: 'url("blob:ride")', backgroundPosition: '100% 0%' } },
          ],
        }),
      ],
    })

    const tiles = screen.getByTestId('timeline-filmstrip').children
    expect(tiles).toHaveLength(2)
    expect(tiles[1]).toHaveStyle({ left: '85px', width: '85px', backgroundPosition: '100% 0%' })
  })

  test('a video segment without a ready filmstrip draws no tiles', () => {
    renderVideoLane()

    expect(screen.queryByTestId('timeline-filmstrip')).not.toBeInTheDocument()
  })

  test('renders activity source, filename, and duration when the lane model allows text', () => {
    renderLane()

    expect(screen.getByText('FIT')).toBeInTheDocument()
    expect(screen.getByText('ride.fit')).toBeInTheDocument()
    expect(screen.getByText('00:20')).toBeInTheDocument()
  })

  test('shades the trimmed-off parts of the activity', () => {
    renderLane({ trimMasks: [{ key: 'start', style: { left: '0%', width: '10%' } }] })

    expect(screen.getByTestId('timeline-activity-trim-mask')).toHaveStyle({ left: '0%', width: '10%' })
  })

  test('hides text inside clips when the lane model says text is not visible', () => {
    renderLane({ showText: false })

    expect(screen.queryByText('FIT')).not.toBeInTheDocument()
    expect(screen.queryByText('ride.fit')).not.toBeInTheDocument()
    expect(screen.queryByText('00:20')).not.toBeInTheDocument()
  })

  test('renders the supplied tooltip model outside the clip mask', () => {
    renderLane({
      tooltip: {
        id: 'tooltip-activity',
        isVisible: true,
        style: { left: '50%' },
      },
    })

    const tooltip = screen.getByRole('tooltip')
    expect(tooltip).toHaveTextContent('ride.fit')
    expect(tooltip).toHaveTextContent('Duration')
    expect(tooltip).toHaveTextContent('00:20')
    expect(tooltip.closest('[data-testid="timeline-lane-clip-mask"]')).toBeNull()
  })

  test('uses supplied clip handlers so clip body events do not bubble', () => {
    const onClick = vi.fn()
    const onPointerDown = vi.fn()
    const stopEvent = (event) => event.stopPropagation()

    render(
      <div onClick={onClick} onPointerDown={onPointerDown}>
        <TimelineLane
          lane={createLane({
            clipProps: {
              onClick: stopEvent,
              onPointerDown: stopEvent,
            },
          })}
        />
      </div>,
    )

    const clip = screen.getByLabelText('ride.fit')
    fireEvent.pointerDown(clip, { button: 0, clientX: 25, pointerId: 1 })
    fireEvent.click(clip)

    expect(onPointerDown).not.toHaveBeenCalled()
    expect(onClick).not.toHaveBeenCalled()
  })
})

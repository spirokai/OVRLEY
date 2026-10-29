import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vite-plus/test'
import TimelineLane from '@/features/player/components/TimelineLane'

function createLane(overrides = {}) {
  return {
    ariaLabel: 'Activity clip lane',
    clipClassName: 'bg-primary/80',
    clipContentClassName: 'translate-y-[0.04rem]',
    clipProps: {},
    clipStyle: { left: '0%', width: '50%' },
    durationLabel: '00:20',
    formatLabel: 'FIT',
    highlightStyle: null,
    icon: null,
    id: 'activity',
    isVideo: false,
    isVisible: true,
    label: 'ride.fit',
    showText: true,
    sourceColumnWidth: '1.5rem',
    textClassName: 'text-background',
    tooltip: {
      id: 'tooltip-activity',
      isVisible: false,
      style: { left: '50%' },
    },
    ...overrides,
  }
}

function renderLane(overrides = {}) {
  return render(<TimelineLane lane={createLane(overrides)} />)
}

describe('TimelineLane', () => {
  test('renders activity source, filename, and duration when the lane model allows text', () => {
    renderLane()

    expect(screen.getByText('FIT')).toBeInTheDocument()
    expect(screen.getByText('ride.fit')).toBeInTheDocument()
    expect(screen.getByText('00:20')).toBeInTheDocument()
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

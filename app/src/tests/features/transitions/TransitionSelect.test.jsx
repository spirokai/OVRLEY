import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { JoinTransitionControl, TransitionSelect } from '@/features/transitions'

describe('TransitionSelect', () => {
  test('picks a transition from animated previews, filtered by name', () => {
    const onSelect = vi.fn()
    render(<TransitionSelect value="default" label="fade, 1 s" previewType="fade" defaultLabel="Default (fade, 1 s)" onSelect={onSelect} />)

    fireEvent.click(screen.getByRole('button', { name: /fade, 1 s/ }))
    const picker = screen.getByRole('listbox', { name: 'Transitions' })
    expect(screen.getByRole('option', { name: 'Default (fade, 1 s)' })).toHaveAttribute('aria-selected', 'true')
    expect(picker.querySelectorAll('[style*="transition-previews"]')).toHaveLength(58)

    fireEvent.change(screen.getByLabelText('Filter transitions'), { target: { value: 'circle' } })
    expect(screen.getAllByRole('option').map((option) => option.getAttribute('aria-label'))).toEqual([
      'Default (fade, 1 s)',
      'No transition',
      'circlecrop',
      'circleopen',
      'circleclose',
    ])
    fireEvent.click(screen.getByRole('option', { name: 'circleopen' }))
    expect(onSelect).toHaveBeenCalledExactlyOnceWith('circleopen')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })

  test('the project default itself offers no default option', () => {
    render(<TransitionSelect value="none" label="No transition" previewType={null} onSelect={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: /No transition/ }))
    expect(screen.getByRole('option', { name: 'No transition' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByRole('option', { name: /Default/ })).not.toBeInTheDocument()
  })
})

describe('JoinTransitionControl', () => {
  test('commits durations snapped to the join frames and shows invalid ones and placements as alerts', () => {
    const commitDuration = vi.fn()
    const model = {
      isAvailable: true,
      value: 'fade',
      label: 'fade, 1 s',
      defaultLabel: 'Default',
      previewType: 'fade',
      durationSeconds: 1,
      durationFps: { num: 25, den: 1 },
      error: null,
      onSelect: vi.fn(),
      commitDuration,
    }
    const { rerender } = render(<JoinTransitionControl id="join" model={model} />)
    const duration = screen.getByLabelText('Transition duration')
    fireEvent.change(duration, { target: { value: '0' } })
    fireEvent.blur(duration)
    expect(commitDuration).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a duration above 0 seconds')
    // 25.75 frames at 25 fps round to 26 frames.
    fireEvent.change(duration, { target: { value: '1.03' } })
    fireEvent.blur(duration)
    expect(commitDuration).toHaveBeenCalledExactlyOnceWith(1.04)

    rerender(<JoinTransitionControl id="join" model={{ ...model, durationSeconds: null, error: 'Too long for the video pieces it joins' }} />)
    fireEvent.click(screen.getByRole('button', { name: /fade, 1 s/ }))
    fireEvent.click(screen.getByRole('option', { name: 'Default' }))
    expect(model.onSelect).toHaveBeenCalledWith('default')
    expect(screen.getByRole('alert')).toHaveTextContent('Too long for the video pieces it joins')
  })
})

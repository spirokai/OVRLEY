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
    expect(picker.querySelectorAll('img')).toHaveLength(58)

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
  test('shows invalid durations and placements as alerts', () => {
    const commitDuration = vi.fn((text) => (text === '0' ? 'Enter a duration above 0 seconds' : null))
    const model = {
      isAvailable: true,
      value: 'fade',
      label: 'fade, 1 s',
      defaultLabel: 'Default',
      previewType: 'fade',
      durationInput: '1',
      error: null,
      onSelect: vi.fn(),
      commitDuration,
    }
    const { rerender } = render(<JoinTransitionControl id="join" model={model} />)
    const duration = screen.getByLabelText('Transition duration (seconds)')
    fireEvent.change(duration, { target: { value: '0' } })
    fireEvent.blur(duration)
    expect(commitDuration).toHaveBeenCalledWith('0')
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a duration above 0 seconds')

    rerender(<JoinTransitionControl id="join" model={{ ...model, durationInput: null, error: 'Too long for the video pieces it joins' }} />)
    fireEvent.click(screen.getByRole('button', { name: /fade, 1 s/ }))
    fireEvent.click(screen.getByRole('option', { name: 'Default' }))
    expect(model.onSelect).toHaveBeenCalledWith('default')
    expect(screen.getByRole('alert')).toHaveTextContent('Too long for the video pieces it joins')
  })
})

/**
 * Behavior tests for the shared seconds input: clock entry, the seconds readout,
 * frame snapping on commit, and validation.
 */

import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vitest'
import { SecondsInput } from '@/components/ui/seconds-input'

const PAL = { num: 25, den: 1 }

function renderInput(props) {
  const onCommit = props.onCommit ?? vi.fn(() => null)
  const view = render(<SecondsInput aria-label="Start" value={0} fps={PAL} {...props} onCommit={onCommit} />)
  return { ...view, input: screen.getByRole('textbox', { name: 'Start' }), onCommit }
}

describe('SecondsInput', () => {
  test('shows the value as a clock, ignores anything but a time, and reads the draft in seconds', async () => {
    const user = userEvent.setup()
    const { input } = renderInput({ value: 3700 })
    expect(input).toHaveValue('01:01:40')
    expect(screen.getByText('3700 s')).toBeInTheDocument()

    await user.clear(input)
    await user.type(input, '5a:3x0')
    expect(input).toHaveValue('5:30')
    expect(screen.getByText('330 s')).toBeInTheDocument()

    await user.clear(input)
    await user.type(input, '330')
    expect(screen.getByText('330 s')).toBeInTheDocument()
  })

  test('commits on Enter or leaving the field, snapped to the nearest frame', async () => {
    const user = userEvent.setup()
    const { input, onCommit, rerender } = renderInput({ value: 0 })

    await user.clear(input)
    await user.type(input, '5:00.03{Enter}')
    expect(onCommit).toHaveBeenCalledExactlyOnceWith(300.04)

    // The field shows the stored value once the store has it.
    rerender(<SecondsInput aria-label="Start" value={300.04} fps={PAL} onCommit={onCommit} />)
    expect(input).toHaveValue('05:00.04')

    await user.clear(input)
    await user.type(input, '12')
    fireEvent.blur(input)
    expect(onCommit).toHaveBeenLastCalledWith(12)
  })

  test('does not commit an unchanged value and discards a draft on Escape', async () => {
    const user = userEvent.setup()
    const { input, onCommit } = renderInput({ value: 10 })

    await user.clear(input)
    await user.type(input, '00:10.01{Enter}')
    expect(onCommit).not.toHaveBeenCalled()
    expect(input).toHaveValue('00:10')

    await user.clear(input)
    await user.type(input, '42{Escape}')
    expect(input).toHaveValue('00:10')
    fireEvent.blur(input)
    expect(onCommit).not.toHaveBeenCalled()
  })

  test('rejects incomplete or invalid times, negative values unless signed, and durations of 0', async () => {
    const user = userEvent.setup()
    const { input, onCommit, unmount } = renderInput({ value: 5 })

    await user.clear(input)
    await user.type(input, '-.{Enter}')
    expect(input).toHaveValue('.')
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a time like 05:30 or a number of seconds')
    expect(input).toHaveAttribute('aria-invalid', 'true')
    await user.clear(input)
    await user.type(input, '5:75{Enter}')
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a time like 05:30 or a number of seconds')
    unmount()

    const signed = renderInput({ value: 5, signed: true })
    await user.clear(signed.input)
    await user.type(signed.input, '-00:02{Enter}')
    expect(signed.onCommit).toHaveBeenCalledExactlyOnceWith(-2)
    signed.unmount()

    const duration = renderInput({ value: 1, mode: 'duration' })
    await user.clear(duration.input)
    await user.type(duration.input, '0{Enter}')
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a duration above 0 seconds')
    await user.clear(duration.input)
    await user.type(duration.input, '0.01{Enter}')
    expect(duration.onCommit).toHaveBeenCalledExactlyOnceWith(0.04)
    expect(onCommit).not.toHaveBeenCalled()
  })

  test('keeps the draft and shows the reason when the value is rejected', async () => {
    const user = userEvent.setup()
    const { input } = renderInput({ value: 5, onCommit: vi.fn(() => 'A video must be loaded first') })

    await user.clear(input)
    await user.type(input, '8{Enter}')
    expect(input).toHaveValue('8')
    expect(screen.getByRole('alert')).toHaveTextContent('A video must be loaded first')
  })

  test('steps by the given amount, snapped to the frame grid', async () => {
    const user = userEvent.setup()
    const { onCommit } = renderInput({ value: 10, signed: true, step: 0.1 })

    await user.click(screen.getByRole('button', { name: 'Increase value' }))
    expect(onCommit).toHaveBeenLastCalledWith(10.12)
    await user.click(screen.getByRole('button', { name: 'Decrease value' }))
    expect(onCommit).toHaveBeenLastCalledWith(9.92)
  })
})

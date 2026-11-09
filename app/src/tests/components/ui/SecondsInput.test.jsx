/**
 * Behavior tests for the shared seconds input: invalid and rejected values keep the
 * draft and show why.
 */

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vite-plus/test'
import { SecondsInput } from '@/components/ui/seconds-input'

const PAL = { num: 25, den: 1 }

function renderInput(props) {
  const onCommit = props.onCommit ?? vi.fn(() => null)
  const view = render(<SecondsInput aria-label="Start" value={0} fps={PAL} {...props} onCommit={onCommit} />)
  return { ...view, input: screen.getByRole('textbox', { name: 'Start' }), onCommit }
}

describe('SecondsInput', () => {
  test('rejects incomplete or invalid times, negative values unless signed, and durations of 0', async () => {
    const user = userEvent.setup()
    const { input, onCommit, unmount } = renderInput({ value: 5 })

    await user.clear(input)
    await user.type(input, '-.{Enter}')
    expect(input).toHaveValue('.')
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a time like 05:30, a number of seconds, or a sum like 05:30+10')
    expect(input).toHaveAttribute('aria-invalid', 'true')
    await user.clear(input)
    await user.type(input, '5:75{Enter}')
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a time like 05:30, a number of seconds, or a sum like 05:30+10')
    await user.clear(input)
    await user.type(input, '3-5{Enter}')
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a time of 0 seconds or more')
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
})

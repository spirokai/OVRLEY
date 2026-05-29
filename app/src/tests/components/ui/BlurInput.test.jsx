/**
 * Behavior tests for BlurInput's draft lifecycle.
 *
 * Scene settings and render dialogs rely on this input to hold temporary user
 * edits locally until commit. These specs lock in that contract at the shared
 * component boundary instead of repeating it in each feature.
 */

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vitest'
import { BlurInput } from '@/components/ui/blur-input'

describe('BlurInput draft lifecycle', () => {
  test('preserves an in-progress draft when the committed value changes externally', async () => {
    const user = userEvent.setup()
    const handleChange = vi.fn()
    const { rerender } = render(<BlurInput aria-label="FPS" value="30" onChange={handleChange} />)

    const input = screen.getByRole('textbox', { name: 'FPS' })

    await user.clear(input)
    await user.type(input, '25')
    expect(input).toHaveValue('25')

    rerender(<BlurInput aria-label="FPS" value="60" onChange={handleChange} />)

    expect(input).toHaveValue('25')
  })

  test('adopts external committed values when no dirty draft is active', () => {
    const { rerender } = render(<BlurInput aria-label="FPS" value="30" onChange={vi.fn()} />)

    const input = screen.getByRole('textbox', { name: 'FPS' })

    rerender(<BlurInput aria-label="FPS" value="60" onChange={vi.fn()} />)

    expect(input).toHaveValue('60')
  })
})

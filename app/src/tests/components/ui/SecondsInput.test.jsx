import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vite-plus/test'
import { SecondsInput } from '@/components/ui/seconds-input'

describe('SecondsInput', () => {
  test('a seconds display shows one decimal and the SEC unit, and keeps it after a commit', () => {
    const onCommit = vi.fn(() => null)
    const { rerender } = render(
      <SecondsInput value={2} fps={{ num: 30, den: 1 }} mode="duration" display="seconds" onCommit={onCommit} aria-label="Duration" />,
    )
    const input = screen.getByRole('textbox', { name: 'Duration' })
    expect(input).toHaveValue('2.0')
    expect(screen.getByText('SEC')).toBeInTheDocument()

    fireEvent.change(input, { target: { value: '1.25' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    // Snapped to the 30 fps grid: 37.5 frames round to 38, 1.2667 s.
    expect(onCommit).toHaveBeenCalledWith(38 / 30)
    rerender(<SecondsInput value={38 / 30} fps={{ num: 30, den: 1 }} mode="duration" display="seconds" onCommit={onCommit} aria-label="Duration" />)
    expect(input).toHaveValue('1.3')
  })
})

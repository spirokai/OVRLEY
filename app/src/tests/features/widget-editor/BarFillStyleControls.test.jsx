import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeAll, describe, expect, test, vi } from 'vite-plus/test'
import { BarFillStyleDetails, BarFillStyleField } from '@/features/widget-editor/components/metricWidget/BarFillStyleControls'

beforeAll(() => {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  HTMLElement.prototype.scrollIntoView = vi.fn()
})

describe('BarFillStyleControls', () => {
  test('seeds count and gap only when switching to bars', async () => {
    const user = userEvent.setup()
    const suggestBarGeometry = vi.fn(() => ({ count: 11, gap: 4, maxGap: 12, cornerRadiusMax: 3 }))
    const updateVariant = vi.fn()
    render(
      <BarFillStyleField
        data={{ track_fill_style: 'fill', track_fill_flat: false, track_corner_radius: 6 }}
        suggestBarGeometry={suggestBarGeometry}
        updateVariant={updateVariant}
      />,
    )

    screen.getByRole('combobox').focus()
    await user.keyboard('[Space][ArrowDown][Enter]')

    expect(suggestBarGeometry).toHaveBeenCalledOnce()
    expect(updateVariant).toHaveBeenCalledWith({ track_fill_style: 'bars', bar_count: 11, bar_gap: 4, track_corner_radius: 3 })
  })

  test('renders the controls for an existing bars configuration', () => {
    render(
      <BarFillStyleDetails
        data={{ track_fill_style: 'bars', bar_count: 8, bar_gap: 4 }}
        barGapMax={42}
        getCornerRadiusMax={vi.fn()}
        updateVariant={vi.fn()}
      />,
    )

    expect(screen.getByText('Bar Count')).toBeInTheDocument()
    expect(screen.getByText('Bar Gap')).toBeInTheDocument()
    const [barCount, barGap] = screen.getAllByRole('slider')
    expect(barCount).toHaveAttribute('aria-valuemax', '64')
    expect(barGap).toHaveAttribute('aria-valuemax', '42')
  })
})

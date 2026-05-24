/**
 * Tests for WidgetButtonGrid — verifies the widget button grid renders and handles clicks.
 */

import { describe, test, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { WidgetButtonGrid } from '@/features/widget-drawer/components/WidgetButtonGrid'

describe('WidgetButtonGrid', () => {
  test('renders a button for each widget type including Wave 1 standard metrics', () => {
    render(<WidgetButtonGrid onAddWidget={() => {}} />)

    const buttons = screen.getAllByRole('button')
    expect(buttons).toHaveLength(21)
  })

  test('each button displays an icon', () => {
    const { container } = render(<WidgetButtonGrid onAddWidget={() => {}} />)

    const buttons = container.querySelectorAll('button')
    buttons.forEach((button) => {
      expect(button.querySelector('svg')).toBeInTheDocument()
    })
  })

  test('each button displays the widget drawer label, including Wave 1 standard metrics', () => {
    render(<WidgetButtonGrid onAddWidget={() => {}} />)

    expect(screen.getByText('HR')).toBeInTheDocument()
    expect(screen.getByText('Map')).toBeInTheDocument()
    expect(screen.getByText('Temp.')).toBeInTheDocument()
    expect(screen.getByText('G-Force')).toBeInTheDocument()
    expect(screen.getByText('Air Press.')).toBeInTheDocument()
    expect(screen.getByText('GCT')).toBeInTheDocument()
    expect(screen.getByText('L/R Bal.')).toBeInTheDocument()
    expect(screen.getByText('Stride')).toBeInTheDocument()
    expect(screen.getByText('S/R')).toBeInTheDocument()
    expect(screen.getByText('V. Speed')).toBeInTheDocument()
    expect(screen.getByText('Gear')).toBeInTheDocument()
    expect(screen.getByText('V. Osc.')).toBeInTheDocument()
  })

  test('clicking a button calls onAddWidget with the correct type', async () => {
    const onAddWidget = vi.fn()
    const user = userEvent.setup()
    render(<WidgetButtonGrid onAddWidget={onAddWidget} />)

    await user.click(screen.getByText('Speed').closest('button'))

    expect(onAddWidget).toHaveBeenCalledWith('speed')
  })

  test('clicking a button does not auto-close the drawer', async () => {
    const onAddWidget = vi.fn()
    const user = userEvent.setup()
    render(<WidgetButtonGrid onAddWidget={onAddWidget} />)

    await user.click(screen.getByText('HR').closest('button'))

    // onAddWidget is called but nothing else — drawer state is managed externally
    expect(onAddWidget).toHaveBeenCalledTimes(1)
  })
})

/**
 * Tests for WidgetButtonGrid — verifies the widget button grid renders and handles clicks.
 */

import { describe, test, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { WidgetButtonGrid } from '@/features/widget-drawer/components/WidgetButtonGrid'

describe('WidgetButtonGrid', () => {
  test('renders a button for each widget type', () => {
    render(<WidgetButtonGrid onAddWidget={() => {}} />)

    const buttons = screen.getAllByRole('button')
    expect(buttons).toHaveLength(10)
  })

  test('each button displays an icon', () => {
    const { container } = render(<WidgetButtonGrid onAddWidget={() => {}} />)

    const buttons = container.querySelectorAll('button')
    buttons.forEach((button) => {
      expect(button.querySelector('svg')).toBeInTheDocument()
    })
  })

  test('each button displays the widget drawer label', () => {
    render(<WidgetButtonGrid onAddWidget={() => {}} />)

    expect(screen.getByText('HR')).toBeInTheDocument()
    expect(screen.getByText('Map')).toBeInTheDocument()
    expect(screen.getByText('Temp.')).toBeInTheDocument()
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

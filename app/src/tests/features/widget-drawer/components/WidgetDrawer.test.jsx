/**
 * Tests for WidgetDrawer — verifies the drawer renders and responds to interaction.
 */

import { describe, test, expect, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import useStore from '@/store/useStore'
import { WidgetDrawer } from '@/features/widget-drawer/components/WidgetDrawer'

beforeEach(() => {
  useStore.setState({
    widgetDrawerOpen: false,
  })
})

describe('WidgetDrawer', () => {
  test('renders a tab with Grid3X3 icon when collapsed', () => {
    render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })
    expect(tab).toBeInTheDocument()

    const icon = tab.querySelector('svg')
    expect(icon).toBeInTheDocument()
  })

  test('clicking the tab opens the drawer', async () => {
    const user = userEvent.setup()
    render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })
    expect(tab).toHaveAttribute('aria-label', 'Open widget drawer')

    await user.click(tab)

    expect(tab).toHaveAttribute('aria-label', 'Close widget drawer')
  })

  test('clicking the tab again closes the drawer', async () => {
    const user = userEvent.setup()
    render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })

    await user.click(tab)
    expect(tab).toHaveAttribute('aria-label', 'Close widget drawer')

    await user.click(tab)
    expect(tab).toHaveAttribute('aria-label', 'Open widget drawer')
  })

  test('pressing Escape closes the drawer when open', async () => {
    const user = userEvent.setup()
    render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })

    await user.click(tab)
    expect(tab).toHaveAttribute('aria-label', 'Close widget drawer')

    await user.keyboard('{Escape}')

    expect(tab).toHaveAttribute('aria-label', 'Open widget drawer')
  })

  test('pressing Escape does nothing when drawer is closed', async () => {
    const user = userEvent.setup()
    render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })
    expect(tab).toHaveAttribute('aria-label', 'Open widget drawer')

    await user.keyboard('{Escape}')

    expect(tab).toHaveAttribute('aria-label', 'Open widget drawer')
  })

  test('does not render a modal backdrop when open', async () => {
    const user = userEvent.setup()
    const { container } = render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })
    await user.click(tab)

    // No overlay/backdrop element should exist
    const overlays = container.querySelectorAll('[role="dialog"], [role="presentation"]')
    expect(overlays.length).toBe(0)

    // No fixed full-screen backdrop div
    const backdrop = container.querySelector('.fixed.inset-0')
    expect(backdrop).toBeNull()
  })
})

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
  test('renders a tab with WIDGETS label when collapsed', () => {
    render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })
    expect(tab).toBeInTheDocument()
    expect(tab).toHaveTextContent('WIDGETS')
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

  test('clicking the backdrop closes the drawer', async () => {
    const user = userEvent.setup()
    render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })
    await user.click(tab)

    const backdrop = screen.getByTestId('widget-drawer-backdrop')
    await user.click(backdrop)

    expect(tab).toHaveAttribute('aria-label', 'Open widget drawer')
  })

  test('clicking a widget closes the drawer', async () => {
    const user = userEvent.setup()
    render(<WidgetDrawer />)

    const tab = screen.getByRole('button', { name: /drawer/i })
    await user.click(tab)

    await user.click(screen.getByText('HR').closest('button'))

    expect(tab).toHaveAttribute('aria-label', 'Open widget drawer')
  })
})

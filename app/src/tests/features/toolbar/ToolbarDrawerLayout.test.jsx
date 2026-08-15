import { fireEvent, render } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { ToolbarDrawerLayout } from '@/features/toolbar'

function renderLayout(overrides = {}) {
  const props = {
    activeTool: 'widgets',
    allocationTransitioning: false,
    controlPanel: <button type="button">Control panel action</button>,
    drawerContent: <div>Drawer content</div>,
    pinned: false,
    visible: true,
    workspace: <button type="button">Workspace action</button>,
    dismissOverlay: vi.fn(),
    selectTool: vi.fn(),
    setPinned: vi.fn(),
    handleAllocationTransitionEnd: vi.fn(),
    handleDrawerTransitionEnd: vi.fn(),
    ...overrides,
  }

  return { ...render(<ToolbarDrawerLayout {...props} />), props }
}

describe('ToolbarDrawerLayout', () => {
  test('places one unpinned backdrop above both workspace and control panel', () => {
    const { container, props } = renderLayout()
    const backdrop = container.querySelector('[data-slot="left-drawer-backdrop"]')

    expect(backdrop).toHaveClass('absolute', 'right-0', 'z-55')
    expect(backdrop).toHaveStyle({ left: '3rem' })

    fireEvent.click(backdrop)
    expect(props.dismissOverlay).toHaveBeenCalledOnce()
  })

  test('places the transition blocker above z-50 workspace controls', () => {
    const { container } = renderLayout({ allocationTransitioning: true, pinned: true })

    expect(container.querySelector('[data-slot="workspace-transition-blocker"]')).toHaveClass('z-55')
  })

  test('does not render a backdrop for a pinned drawer', () => {
    const { container } = renderLayout({ pinned: true })

    expect(container.querySelector('[data-slot="left-drawer-backdrop"]')).not.toBeInTheDocument()
  })
})

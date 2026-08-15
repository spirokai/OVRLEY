import { beforeEach, describe, expect, test } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { WidgetDrawerContent as WidgetDrawerView } from '@/features/widget-drawer'
import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { BACKDROP_RECTANGLE_DEFAULTS } from '@/lib/widget/standard-widgets'
import { cloneSerializable, DEFAULT_CONFIG } from '@/store/store-utils'
import useStore from '@/store/useStore'

function WidgetDrawerContent() {
  const widgetLiveEdits = useWidgetDraftState()
  return <WidgetDrawerView widgetLiveEdits={widgetLiveEdits} />
}

beforeEach(() => {
  useStore.setState({
    config: cloneSerializable(DEFAULT_CONFIG),
    leftDrawerPinned: false,
    leftDrawerVisible: true,
    selectedWidgetId: null,
    selectedWidgetIds: [],
  })
})

async function addBackdropRectangle(user) {
  await user.click(screen.getByText('Backdrop').closest('button'))
  await user.click(screen.getByRole('button', { name: 'Rectangle' }))
}

describe('WidgetDrawerContent', () => {
  test('creates a rectangle backdrop from manifest defaults', async () => {
    const user = userEvent.setup()
    render(<WidgetDrawerContent />)

    await addBackdropRectangle(user)

    const [backdrop] = useStore.getState().config.backdrops
    const { width, height, corner_radius, round_top_left, round_top_right, round_bottom_left, round_bottom_right, ...sharedDefaults } =
      BACKDROP_RECTANGLE_DEFAULTS
    expect(backdrop).toMatchObject({
      ...sharedDefaults,
      display_variants: {
        rectangle: { width, height, corner_radius, round_top_left, round_top_right, round_bottom_left, round_bottom_right },
      },
    })
    expect(backdrop.id).toMatch(/^widget-\d+$/)
    expect(useStore.getState().selectedWidgetId).toBe(backdrop.id)
  })

  test('successful creation dismisses an unpinned drawer', async () => {
    const user = userEvent.setup()
    render(<WidgetDrawerContent />)

    await addBackdropRectangle(user)

    expect(useStore.getState().leftDrawerVisible).toBe(false)
  })

  test('successful creation leaves a pinned drawer visible', async () => {
    useStore.setState({ leftDrawerPinned: true })
    const user = userEvent.setup()
    render(<WidgetDrawerContent />)

    await addBackdropRectangle(user)

    expect(useStore.getState().leftDrawerVisible).toBe(true)
  })
})

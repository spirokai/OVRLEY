import { describe, expect, test, vi } from 'vitest'
import { useScaleHandlers } from '@/features/overlay-editor/hooks/useScaleHandlers'

describe('useScaleHandlers', () => {
  test('publishes live text metric values while the captured layout owns geometry', () => {
    const widget = {
      id: 'speed-1',
      category: 'values',
      type: 'speed',
      data: {
        display_type: 'text',
        x: 100,
        y: 200,
        font_size: 40,
        icon_size: 20,
        icon_offset_x: 2,
        icon_offset_y: -4,
      },
    }
    const target = document.createElement('div')
    target.style.left = '95px'
    target.style.top = '197px'
    target.style.width = '120px'
    target.style.height = '48px'
    target.dataset.widgetBoundsLeft = '-5'
    target.dataset.widgetBoundsTop = '-3'
    target.dataset.widgetBoundsRight = '115'
    target.dataset.widgetBoundsBottom = '45'

    const interactionStartRef = { current: null }
    const draftWidgetsRef = { current: {} }
    const commitWidgetUpdate = vi.fn()
    const setLiveWidgetDraft = (widgetId, data, layout) => {
      draftWidgetsRef.current[widgetId] = { data, layout }
    }
    const handlers = useScaleHandlers({
      interactionStartRef,
      draftWidgetsRef,
      selectedWidget: widget,
      selectedTarget: target,
      globalScale: 1,
      setLiveWidgetDraft,
      commitWidgetUpdate,
      clearWidgetDraft: vi.fn(),
      beginWidgetInteraction: vi.fn(),
      endWidgetInteraction: vi.fn(),
    })

    handlers.onScaleStart({ dragStart: { set: vi.fn() }, target })
    handlers.onScale({ scale: [1.25, 1.25], drag: { beforeTranslate: [0, 0] }, target })
    expect(draftWidgetsRef.current[widget.id].data.x).toBe(100)

    handlers.onScale({ scale: [1.5, 1.5], drag: { beforeTranslate: [10, 15] }, target })

    expect(draftWidgetsRef.current[widget.id]).toEqual({
      data: {
        font_size: 60,
        icon_size: 30,
        icon_offset_x: 3,
        icon_offset_y: -6,
        x: 110,
        y: 216.5,
      },
      layout: {
        mode: 'scale',
        left: 95,
        top: 197,
        width: 120,
        height: 48,
        scaleFactor: 1.5,
        globalScale: 1,
        translateX: 10,
        translateY: 15,
        transformOriginX: 5,
        rotation: 0,
      },
    })
    expect(target.style.width).toBe('120px')
    expect(target.style.height).toBe('48px')
    expect(target.style.transform).toBe('translate(10px, 15px) scale(1.5)')

    handlers.onScaleEnd()

    expect(commitWidgetUpdate).toHaveBeenCalledWith(widget.id, {
      x: 110,
      y: 217,
      font_size: 60,
      icon_size: 30,
      icon_offset_x: 3,
      icon_offset_y: -6,
    })
  })
})

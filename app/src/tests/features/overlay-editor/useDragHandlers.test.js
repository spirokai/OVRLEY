import { describe, expect, test, vi } from 'vitest'
import { useDragHandlers } from '@/features/overlay-editor/hooks/useDragHandlers'

describe('useDragHandlers', () => {
  test('commits an intrinsic widget anchor plus drag delta instead of its visual left', () => {
    const widget = {
      id: 'speed-1',
      category: 'values',
      type: 'speed',
      data: { content_alignment: 'right', display_type: 'text', x: 300, y: 100 },
    }
    const target = document.createElement('div')
    target.style.left = '180px'
    target.style.top = '100px'
    target.style.width = '120px'
    target.style.height = '50px'

    const interactionStartRef = { current: null }
    const draftWidgetsRef = { current: {} }
    const commitWidgetUpdate = vi.fn()
    const handlers = useDragHandlers({
      interactionStartRef,
      draftWidgetsRef,
      selectedWidget: widget,
      selectedWidgets: [widget],
      globalScale: 1,
      effectiveSelectedWidgetIds: [widget.id],
      setLiveWidgetDraft: (widgetId, data, layout) => {
        draftWidgetsRef.current[widgetId] = { data, layout }
      },
      setLiveWidgetDraftsBatch: vi.fn(),
      commitWidgetUpdate,
      commitWidgetUpdates: vi.fn(),
      clearWidgetDraft: vi.fn(),
      clearWidgetDrafts: vi.fn(),
      setIsGroupDragActive: vi.fn(),
      setGroupDragSelectionIds: vi.fn(),
      beginWidgetInteraction: vi.fn(),
      endWidgetInteraction: vi.fn(),
    })

    handlers.onDragStart({ target })
    handlers.onDrag({ beforeTranslate: [20, 5], inputEvent: {}, target })
    handlers.onDragEnd()

    expect(commitWidgetUpdate).toHaveBeenCalledWith(widget.id, { x: 320, y: 105 })
  })
})

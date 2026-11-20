import { renderHook } from '@testing-library/react'
import { expect, test, vi } from 'vite-plus/test'
import { useMapViewGesture } from '@/features/overlay-editor/hooks/useMapViewGesture'

test('Escape leaves the view mode while a sidebar field still has focus', () => {
  const setMapViewWidgetId = vi.fn()
  const map = { id: 'map', type: 'map', data: { camera_mode: 'fixed' } }
  renderHook(() =>
    useMapViewGesture({
      widget: map,
      node: document.createElement('div'),
      sceneElement: document.createElement('div'),
      widgetMap: { map },
      setLiveWidgetDraft: vi.fn(),
      commitWidgetUpdate: vi.fn(),
      setMapViewWidgetId,
    }),
  )
  const field = document.body.appendChild(document.createElement('input'))
  field.focus()

  field.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Escape' }))

  expect(setMapViewWidgetId).toHaveBeenCalledWith(null)
  field.remove()
})

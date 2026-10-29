import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import useWidgetDraftState, { useWidgetDraftView } from '@/features/overlay-editor/hooks/useWidgetDraftState'

describe('live widget draft publication', () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] }))
  afterEach(() => vi.useRealTimers())

  test('publishes the latest merged drafts once per frame while interaction refs update immediately', () => {
    const { result } = renderHook(() => {
      const controller = useWidgetDraftState()
      return { controller, view: useWidgetDraftView(controller) }
    })
    const listener = vi.fn()
    const unsubscribe = result.current.controller.subscribe(listener)
    const initialSnapshot = result.current.controller.getSnapshot()

    act(() => {
      for (let font_size = 30; font_size <= 50; font_size++) result.current.controller.setLiveWidgetDraft('speed', { font_size })
      result.current.controller.setLiveWidgetDrafts({ speed: { icon_size: 20 }, other: { x: 10 } })
    })

    expect(result.current.controller.getWidgetDraft('speed')).toEqual({ font_size: 50, icon_size: 20 })
    expect(result.current.controller.getSnapshot()).toBe(initialSnapshot)
    expect(listener).not.toHaveBeenCalled()

    act(() => vi.advanceTimersToNextFrame())
    expect(listener).toHaveBeenCalledTimes(1)
    expect(result.current.view.liveWidgetDrafts.speed).toEqual({ font_size: 50, icon_size: 20 })
    expect(result.current.view.liveWidgetDrafts.other).toEqual({ x: 10 })

    act(() => result.current.controller.setLiveWidgetDraft('speed', { font_size: 60 }))
    act(() => vi.advanceTimersToNextFrame())
    expect(listener).toHaveBeenCalledTimes(2)
    expect(result.current.view.liveWidgetDrafts.speed.font_size).toBe(60)
    unsubscribe()
  })

  test.each(['clearWidgetDraft', 'clearWidgetDrafts', 'resetWidgetDrafts'])(
    '%s cancels pending publication without restoring cleared drafts',
    (method) => {
      const { result } = renderHook(useWidgetDraftState)
      const listener = vi.fn()
      const unsubscribe = result.current.subscribe(listener)

      result.current.setLiveWidgetDraft('speed', { font_size: 50 })
      result.current[method](method === 'clearWidgetDrafts' ? ['speed'] : 'speed')

      expect(result.current.getSnapshot().liveWidgetDrafts).toEqual({})
      expect(vi.getTimerCount()).toBe(0)
      expect(listener).toHaveBeenCalledTimes(1)
      act(() => vi.advanceTimersToNextFrame())
      expect(listener).toHaveBeenCalledTimes(1)
      unsubscribe()
    },
  )

  test('unmounting the last subscriber cancels its pending frame', () => {
    const { result, unmount } = renderHook(() => {
      const controller = useWidgetDraftState()
      useWidgetDraftView(controller)
      return controller
    })

    act(() => result.current.setLiveWidgetDraft('speed', { font_size: 50 }))
    expect(vi.getTimerCount()).toBe(1)
    unmount()
    expect(vi.getTimerCount()).toBe(0)
  })
})

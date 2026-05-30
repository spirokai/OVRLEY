import { describe, expect, test, beforeEach } from 'vitest'
import useStore from '@/store/useStore'

describe('setSelectedSecond', () => {
  beforeEach(() => {
    useStore.setState({
      selectedSecond: 0,
      startSecond: 0,
      endSecond: 73,
    })
  })

  test('sets the selectedSecond state', () => {
    useStore.getState().setSelectedSecond(42.5)
    expect(useStore.getState().selectedSecond).toBe(42.5)
  })

  test('handles finite numbers', () => {
    useStore.getState().setSelectedSecond(10)
    expect(useStore.getState().selectedSecond).toBe(10)
  })

  test('handles non-finite input by clamping to 0', () => {
    useStore.getState().setSelectedSecond(NaN)
    expect(useStore.getState().selectedSecond).toBe(0)
  })
})

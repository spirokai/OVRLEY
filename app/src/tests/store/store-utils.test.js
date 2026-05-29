/**
 * Characterization tests for store-utils helpers.
 *
 * These tests freeze the existing semantics for cloning, equality comparison,
 * and dirty-state tracking before refactoring away from JSON.stringify.
 */

import { describe, expect, test } from 'vitest'
import { cloneSerializable, DEFAULT_CONFIG, hasSerializableChanged } from '@/store/store-utils'

describe('cloneSerializable', () => {
  test('returns a structurally equal deep copy of a plain object', () => {
    const original = {
      scene: { width: 1920, height: 1080, fps: 30 },
      labels: [{ text: 'Hello', x: 10, y: 20 }],
      values: [],
      plots: [],
    }
    const cloned = cloneSerializable(original)

    expect(cloned).toEqual(original)
    expect(cloned).not.toBe(original)
    expect(cloned.scene).not.toBe(original.scene)
    expect(cloned.labels).not.toBe(original.labels)
    expect(cloned.labels[0]).not.toBe(original.labels[0])
  })

  test('handles null gracefully', () => {
    expect(cloneSerializable(null)).toBeNull()
  })

  test('handles undefined gracefully via structuredClone', () => {
    expect(cloneSerializable(undefined)).toBeUndefined()
  })

  test('handles primitive values', () => {
    expect(cloneSerializable(42)).toBe(42)
    expect(cloneSerializable('hello')).toBe('hello')
    expect(cloneSerializable(true)).toBe(true)
  })

  test('handles arrays', () => {
    const original = [1, { a: 2 }, [3, 4]]
    const cloned = cloneSerializable(original)

    expect(cloned).toEqual(original)
    expect(cloned).not.toBe(original)
    expect(cloned[1]).not.toBe(original[1])
    expect(cloned[2]).not.toBe(original[2])
  })
})

describe('hasSerializableChanged', () => {
  test('returns false for structurally equal plain objects', () => {
    const left = {
      scene: { width: 1920, height: 1080 },
      labels: [{ text: 'A', x: 0 }],
      values: [],
      plots: [],
    }
    const right = {
      scene: { width: 1920, height: 1080 },
      labels: [{ text: 'A', x: 0 }],
      values: [],
      plots: [],
    }

    expect(hasSerializableChanged(left, right)).toBe(false)
  })

  test('returns true when a nested value differs', () => {
    const left = {
      scene: { width: 1920, height: 1080 },
      labels: [],
      values: [],
      plots: [],
    }
    const right = {
      scene: { width: 1280, height: 720 },
      labels: [],
      values: [],
      plots: [],
    }

    expect(hasSerializableChanged(left, right)).toBe(true)
  })

  test('returns true when an array element differs', () => {
    const left = {
      scene: { width: 1920 },
      labels: [{ text: 'A' }],
      values: [],
      plots: [],
    }
    const right = {
      scene: { width: 1920 },
      labels: [{ text: 'B' }],
      values: [],
      plots: [],
    }

    expect(hasSerializableChanged(left, right)).toBe(true)
  })

  test('returns true when one value is null and the other is an object', () => {
    const left = { scene: { width: 1920 } }
    const right = null

    expect(hasSerializableChanged(left, right)).toBe(true)
  })

  test('returns true when keys differ between configs', () => {
    const left = { scene: { width: 1920, height: 1080 } }
    const right = { scene: { width: 1920 } }

    expect(hasSerializableChanged(left, right)).toBe(true)
  })

  test('returns false when comparing primitive values that are equal', () => {
    expect(hasSerializableChanged(42, 42)).toBe(false)
    expect(hasSerializableChanged('hello', 'hello')).toBe(false)
    expect(hasSerializableChanged(true, true)).toBe(false)
    expect(hasSerializableChanged(null, null)).toBe(false)
  })

  test('returns true when comparing primitive values that differ', () => {
    expect(hasSerializableChanged(42, 43)).toBe(true)
    expect(hasSerializableChanged('hello', 'world')).toBe(true)
    expect(hasSerializableChanged(true, false)).toBe(true)
    expect(hasSerializableChanged(null, false)).toBe(true)
  })

  test('detects changes between DEFAULT_CONFIG and a modified config', () => {
    const modifiedConfig = cloneSerializable(DEFAULT_CONFIG)
    modifiedConfig.labels.push({ text: 'New Label', x: 100, y: 50 })

    expect(hasSerializableChanged(DEFAULT_CONFIG, modifiedConfig)).toBe(true)
  })

  test('two independently cloned DEFAULT_CONFIGs are considered unchanged', () => {
    const clone1 = cloneSerializable(DEFAULT_CONFIG)
    const clone2 = cloneSerializable(DEFAULT_CONFIG)

    expect(hasSerializableChanged(clone1, clone2)).toBe(false)
  })
})

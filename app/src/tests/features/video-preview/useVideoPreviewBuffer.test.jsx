import { useRef } from 'react'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { useVideoPreviewBuffer, useVideoPreviewLayerRef } from '@/features/video-preview/hooks/useVideoPreviewBuffer'

let paints
beforeEach(() => {
  paints = new Map()
  let nextId = 0
  vi.stubGlobal(
    'requestAnimationFrame',
    vi.fn((callback) => {
      paints.set(++nextId, callback)
      return nextId
    }),
  )
  vi.stubGlobal(
    'cancelAnimationFrame',
    vi.fn((id) => paints.delete(id)),
  )
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {})
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

function Harness(props) {
  const videoRef = useRef(null)
  const { layers, registerElement } = useVideoPreviewBuffer({ videoRef, ...props })
  return layers.map((layer) => <Layer key={layer.key} layer={layer} registerElement={registerElement} />)
}

function Layer({ layer, registerElement }) {
  const ref = useVideoPreviewLayerRef(layer.key, registerElement)
  return (
    <video
      ref={ref}
      data-testid={layer.key}
      src={layer.src}
      aria-hidden={!layer.isVisible}
      muted={!layer.isActive}
      onLoadedMetadata={layer.onLoadedMetadata}
    />
  )
}

test('keeps chapter audio on the native media element instead of attaching a Web Audio graph', () => {
  const audioContext = vi.fn(function () {
    throw new Error('Preview must preserve native HEVC playback')
  })
  vi.stubGlobal('AudioContext', audioContext)
  const { rerender, unmount } = render(
    <Harness videoSrc="http://localhost/one" importId="one" nextVideoSource={{ key: 'two', src: 'http://localhost/two' }} />,
  )
  rerender(<Harness videoSrc="http://localhost/two" importId="two" nextVideoSource={null} />)
  expect(screen.getByTestId('two').muted).toBe(false)
  unmount()
  expect(audioContext).not.toHaveBeenCalled()
})

function paint() {
  act(() => {
    const pending = [...paints.values()]
    paints.clear()
    for (const callback of pending) callback()
  })
}

function decode(video) {
  Object.defineProperty(video, 'readyState', { configurable: true, value: 2 })
  fireEvent.loadedData(video)
  paint()
  paint()
}

test('holds the outgoing frame until the preloaded next element has decoded a frame', () => {
  const { rerender } = render(
    <Harness videoSrc="http://localhost/one" importId="one" nextVideoSource={{ key: 'two', src: 'http://localhost/two' }} />,
  )
  const first = screen.getByTestId('one')
  const preloaded = screen.getByTestId('two')
  decode(first)
  expect(preloaded.muted).toBe(true)
  expect(preloaded).toHaveAttribute('aria-hidden', 'true')

  rerender(<Harness videoSrc="http://localhost/two" importId="two" nextVideoSource={{ key: 'three', src: 'http://localhost/three' }} />)
  expect(screen.getByTestId('two')).toBe(preloaded)
  expect(first).toHaveAttribute('aria-hidden', 'false')
  expect(first.muted).toBe(true)
  expect(preloaded).toHaveAttribute('aria-hidden', 'true')
  paint()
  paint()
  expect(first).toHaveAttribute('aria-hidden', 'false')

  decode(preloaded)
  expect(preloaded).toHaveAttribute('aria-hidden', 'false')
  expect(preloaded.muted).toBe(false)
  expect(screen.queryByTestId('one')).toBeNull()
  expect(screen.getByTestId('three')).toHaveAttribute('aria-hidden', 'true')
})

test('does not reveal a preloaded frame while a seek is still pending', () => {
  const { rerender } = render(
    <Harness videoSrc="http://localhost/one" importId="one" nextVideoSource={{ key: 'two', src: 'http://localhost/two' }} />,
  )
  const first = screen.getByTestId('one')
  decode(first)
  const second = screen.getByTestId('two')
  Object.defineProperty(second, 'readyState', { configurable: true, value: 2 })
  Object.defineProperty(second, 'seeking', { configurable: true, value: true })
  rerender(<Harness videoSrc="http://localhost/two" importId="two" nextVideoSource={null} />)
  paint()
  paint()
  expect(first).toHaveAttribute('aria-hidden', 'false')
  Object.defineProperty(second, 'seeking', { configurable: true, value: false })
  fireEvent.seeked(second)
  paint()
  paint()
  expect(second).toHaveAttribute('aria-hidden', 'false')
  expect(screen.queryByTestId('one')).toBeNull()
})

test('cancels a stale incoming frame when scrubbing to another chapter', () => {
  const { rerender } = render(
    <Harness videoSrc="http://localhost/one" importId="one" nextVideoSource={{ key: 'two', src: 'http://localhost/two' }} />,
  )
  decode(screen.getByTestId('one'))
  const second = screen.getByTestId('two')
  Object.defineProperty(second, 'readyState', { configurable: true, value: 2 })
  rerender(<Harness videoSrc="http://localhost/two" importId="two" nextVideoSource={null} />)
  rerender(<Harness videoSrc="http://localhost/three" importId="three" nextVideoSource={null} />)
  paint()
  paint()
  expect(screen.getByTestId('one')).toHaveAttribute('aria-hidden', 'false')
  decode(screen.getByTestId('three'))
  expect(screen.getByTestId('three')).toHaveAttribute('aria-hidden', 'false')
})

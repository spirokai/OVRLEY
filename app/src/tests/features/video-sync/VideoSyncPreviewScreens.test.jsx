import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeAll, beforeEach, describe, expect, test, vi } from 'vitest'

const getMapStyleUrlTemplate = vi.hoisted(() => vi.fn())
const mapOptions = vi.hoisted(() => vi.fn())
const markerOptions = vi.hoisted(() => vi.fn())
const eventHandlers = vi.hoisted(() => new Map())
const source = vi.hoisted(() => ({ setData: vi.fn() }))
const getPreference = vi.hoisted(() => vi.fn())
const setPreference = vi.hoisted(() => vi.fn())
const resizeObserver = vi.hoisted(() => ({ callback: null }))
const canvasContainer = vi.hoisted(() => ({ style: { cursor: '' } }))
const map = vi.hoisted(() => {
  let hasCourseSource = false
  return {
    addControl: vi.fn(),
    addLayer: vi.fn(),
    addSource: vi.fn(() => {
      hasCourseSource = true
    }),
    fitBounds: vi.fn(),
    getCanvasContainer: vi.fn(() => canvasContainer),
    getSource: vi.fn(() => (hasCourseSource ? source : null)),
    off: vi.fn((name) => eventHandlers.delete(name)),
    on: vi.fn((name, handler) => eventHandlers.set(name, handler)),
    project: vi.fn((position) => ({ x: (position.lng ?? position[0]) * 10, y: (position.lat ?? position[1]) * 10 })),
    remove: vi.fn(),
    resize: vi.fn(),
    setStyle: vi.fn(() => {
      hasCourseSource = false
    }),
    unproject: vi.fn((point) => ({ lng: point.x / 10, lat: point.y / 10 })),
    reset() {
      hasCourseSource = false
    },
  }
})
const marker = vi.hoisted(() => ({ addTo: vi.fn(), remove: vi.fn(), setLngLat: vi.fn() }))

vi.mock('@/api/backend', () => ({ getMapStyleUrlTemplate }))
vi.mock('@/lib/preferences-store', () => ({ getPreference, setPreference }))

vi.mock('maplibre-gl', () => ({
  LngLatBounds: class {
    extend() {
      return this
    }
  },
  Map: vi.fn(function Map(options) {
    mapOptions(options)
    return map
  }),
  Marker: vi.fn(function Marker(options) {
    markerOptions(options)
    marker.setLngLat.mockReturnValue(marker)
    marker.addTo.mockReturnValue(marker)
    return marker
  }),
  NavigationControl: vi.fn(function NavigationControl() {}),
}))

vi.mock('@/features/video-preview', () => ({
  VideoPreviewSurface: ({ children }) => <div data-testid="video-preview-surface">{children}</div>,
}))

import VideoSyncPreviewScreens from '@/features/video-sync/components/VideoSyncPreviewScreens'

describe('VideoSyncPreviewScreens', () => {
  beforeAll(() => {
    globalThis.ResizeObserver = class ResizeObserver {
      constructor(callback) {
        resizeObserver.callback = callback
      }
      observe() {}
      disconnect() {}
    }
    HTMLElement.prototype.hasPointerCapture = vi.fn(() => false)
    HTMLElement.prototype.setPointerCapture = vi.fn()
    HTMLElement.prototype.releasePointerCapture = vi.fn()
    HTMLElement.prototype.scrollIntoView = vi.fn()
  })

  beforeEach(() => {
    getMapStyleUrlTemplate.mockReset()
    getMapStyleUrlTemplate.mockResolvedValue('http://127.0.0.1:3210/styles/{style}')
    getPreference.mockReset()
    getPreference.mockResolvedValue(undefined)
    setPreference.mockReset()
    setPreference.mockResolvedValue(undefined)
    resizeObserver.callback = null
    canvasContainer.style.cursor = ''
    mapOptions.mockClear()
    markerOptions.mockClear()
    eventHandlers.clear()
    source.setData.mockClear()
    marker.addTo.mockClear()
    marker.remove.mockClear()
    marker.setLngLat.mockClear()
    map.reset()
    for (const value of Object.values(map)) {
      if (typeof value?.mockClear === 'function') value.mockClear()
    }
  })

  test('renders an equal-size MapLibre preview with the activity route and cursor picker', async () => {
    const user = userEvent.setup()
    const onSetCourseLocation = vi.fn()
    render(
      <VideoSyncPreviewScreens
        activity={{
          trim_end_seconds: 4,
          sample_elapsed_seconds: [0, 1, 2, 3, 4],
          sample_course_points: [
            [47.37, 8.53],
            [47.38, 8.54],
            [null, null],
            [47.39, 8.55],
            [47.4, 8.56],
          ],
          speed: [4, 6, 6, 6, 6],
        }}
        detection={{
          availability: { speed: true, heading: false, course: true },
          graphSeries: { speed: [], turning: [] },
          location: { id: 'detected-course-location', type: 'location', time: 0.5 },
          stops: [],
          turns: [],
        }}
        displayScale={0.5}
        onSetCourseLocation={onSetCourseLocation}
        previewSecond={0.5}
        sceneSize={{ width: 1920, height: 1080 }}
        setSceneElement={() => {}}
      />,
    )

    const pair = screen.getByTestId('video-sync-preview-screens')
    const video = screen.getByTestId('video-sync-video-screen')
    const mapScreen = screen.getByTestId('video-sync-map-screen')
    expect(pair).toHaveStyle({ width: '1928px', height: '540px', gap: '8px' })
    expect(video).toHaveStyle({ width: '960px', height: '540px' })
    expect(mapScreen).toHaveStyle({ width: '960px', height: '540px' })
    expect(video.nextElementSibling).toBe(mapScreen)
    expect(screen.getByTestId('video-sync-speed-diagnostic')).toHaveTextContent('18.0')
    await waitFor(() => expect(markerOptions).toHaveBeenCalledWith({ color: '#a855f7', scale: 1.5 }))

    await waitFor(() => expect(map.setStyle).toHaveBeenCalledWith('http://127.0.0.1:3210/styles/liberty'))
    act(() => eventHandlers.get('style.load')())
    expect(map.addSource).toHaveBeenCalledWith(
      'activity-course',
      expect.objectContaining({
        data: expect.objectContaining({
          geometry: {
            type: 'MultiLineString',
            coordinates: [
              [
                [8.53, 47.37],
                [8.54, 47.38],
              ],
              [
                [8.55, 47.39],
                [8.56, 47.4],
              ],
            ],
          },
        }),
      }),
    )

    act(() => eventHandlers.get('mousemove')({ point: { x: 85.35, y: 473.75 } }))
    expect(marker.addTo).toHaveBeenCalledWith(map)
    act(() => eventHandlers.get('click')({ point: { x: 85.35, y: 473.75 } }))
    const setLocationButton = screen.getByRole('button', { name: 'Set location in map' })
    expect(setLocationButton).toHaveClass('text-video-sync-location')
    await user.click(setLocationButton)
    expect(onSetCourseLocation).toHaveBeenCalledWith(0.5)
    expect(screen.queryByRole('button', { name: 'Set location in map' })).not.toBeInTheDocument()
  })

  test('starts in Zurich, loads cached styles, and switches among all OpenFreeMap styles', async () => {
    const user = userEvent.setup()
    render(
      <VideoSyncPreviewScreens
        activity={null}
        detection={null}
        displayScale={1}
        previewSecond={0}
        sceneSize={{ width: 100, height: 100 }}
        setSceneElement={() => {}}
      />,
    )

    await waitFor(() =>
      expect(mapOptions).toHaveBeenCalledWith(
        expect.objectContaining({ center: [8.5417, 47.3769], zoom: 13, style: { version: 8, sources: {}, layers: [] } }),
      ),
    )
    await waitFor(() => expect(map.setStyle).toHaveBeenCalledWith('http://127.0.0.1:3210/styles/liberty'))

    await user.click(screen.getByRole('combobox', { name: 'Map style' }))
    await user.click(screen.getByRole('option', { name: 'Fiord' }))
    await waitFor(() => expect(map.setStyle).toHaveBeenCalledWith('http://127.0.0.1:3210/styles/fiord'))
    expect(setPreference).toHaveBeenCalledWith('sync-map-style', 'fiord')
  })

  test('loads the persisted map style when the preview starts', async () => {
    getPreference.mockResolvedValue('dark')

    render(
      <VideoSyncPreviewScreens
        activity={null}
        detection={null}
        displayScale={1}
        previewSecond={0}
        sceneSize={{ width: 100, height: 100 }}
        setSceneElement={() => {}}
      />,
    )

    await waitFor(() => expect(getPreference).toHaveBeenCalledWith('sync-map-style'))
    await waitFor(() => expect(map.setStyle).toHaveBeenCalledWith('http://127.0.0.1:3210/styles/dark'))
    expect(screen.getByRole('combobox', { name: 'Map style' })).toHaveTextContent('Dark')
  })

  test('resizes the mounted map once while preserving its fitted camera', async () => {
    render(
      <VideoSyncPreviewScreens
        activity={{
          trim_end_seconds: 1,
          sample_elapsed_seconds: [0, 1],
          sample_course_points: [
            [47.37, 8.53],
            [47.38, 8.54],
          ],
          speed: [4, 5],
        }}
        detection={null}
        displayScale={1}
        previewSecond={0}
        sceneSize={{ width: 100, height: 100 }}
        setSceneElement={() => {}}
      />,
    )

    await waitFor(() => expect(map.setStyle).toHaveBeenCalled())
    act(() => eventHandlers.get('style.load')())
    const fitCount = map.fitBounds.mock.calls.length

    act(() => {
      resizeObserver.callback([])
      resizeObserver.callback([])
      resizeObserver.callback([])
    })

    await waitFor(() => expect(map.resize).toHaveBeenCalledOnce())
    expect(map.fitBounds).toHaveBeenCalledTimes(fitCount)
    expect(map.remove).not.toHaveBeenCalled()
  })
})

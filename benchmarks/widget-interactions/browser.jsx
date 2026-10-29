import { Profiler, useLayoutEffect } from 'react'
import { createRoot } from 'react-dom/client'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import useOverlayEditorState from '@/features/overlay-editor/hooks/useOverlayEditorState'
import useWidgetDraftState from '@/features/overlay-editor/hooks/useWidgetDraftState'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import OverlayCanvas from '@/features/overlay-editor/components/OverlayCanvas'
import OverlayMoveable from '@/features/overlay-editor/components/OverlayMoveable'
import { loadWidgetModule } from '@/features/wasm-widgets/utils/previewSession'
import { buildConfigWidgets } from '@/lib/widget/widget-presentation'
import { createInteractionProfiler } from './profile'
import '@/i18n'
import '@/styles/moveable.css'

const profiler = createInteractionProfiler({ rectStacks: new URLSearchParams(location.search).has('rect-stacks') })
const gestures = { drag: 0, resize: 0, commits: 0 }
let currentEditor

function commitConfig(config) {
  gestures.commits++
  useStore.getState().setConfig(config)
}

function gestureHandlers(handlers) {
  return Object.fromEntries(
    Object.entries(handlers).map(([name, handler]) => [
      name,
      (event) =>
        profiler.measure(`gesture.${name}`, () => {
          if (name === 'onDragGroupStart') gestures.drag++
          if (name === 'onResizeGroupStart') gestures.resize++
          return handler(event)
        }),
    ]),
  )
}

function useBenchmarkEditor() {
  const { config, globalDefaults } = useStore(useShallow((state) => ({ config: state.config, globalDefaults: state.globalDefaults })))
  const widgetLiveEdits = useWidgetDraftState()
  // Include the real inspector's draft subscription and widget derivation, without rendering its controls.
  useWidgetManager({ widgetLiveEdits })
  const editor = useOverlayEditorState({
    config,
    globalDefaults,
    widgetLiveEdits,
    onConfigChange: commitConfig,
    editorShell: { activeKeyboardWorkspace: 'editor' },
    zoomLevel: 1,
    gridVisible: false,
    snapToGrid: false,
  })
  useLayoutEffect(() => {
    currentEditor = editor
    profiler.watchMoveable(editor.moveableProps.moveableRef.current)
  })
  return { ...editor, moveableProps: { ...editor.moveableProps, handlers: gestureHandlers(editor.moveableProps.handlers) } }
}

function BenchmarkEditor() {
  const editor = useBenchmarkEditor()
  const { displayScale, viewportRef, scrollViewportRef } = editor.viewport
  return (
    <div className="viewport" ref={viewportRef}>
      <div className="scroll" data-overlay-viewport ref={scrollViewportRef}>
        <div className="stage" ref={editor.setStageElement} onMouseDown={editor.handleSceneMouseDown}>
          <div style={{ position: 'relative', width: editor.sceneSize.width * displayScale, height: editor.sceneSize.height * displayScale }}>
            <div style={{ ...editor.sceneSize, transform: `scale(${displayScale})`, transformOrigin: 'top left' }}>
              <OverlayCanvas
                state={editor}
                backgroundMode="transparent"
                displayScale={displayScale}
                gridVisible={false}
                handleWidgetMouseDown={editor.handleWidgetMouseDown}
              />
              <OverlayMoveable {...editor.moveableProps} />
            </div>
            {editor.widgetBadges.map((badge) => (
              <span key={badge.id} className="badge" style={{ left: badge.left, top: badge.top, transform: `rotate(${badge.rotation}deg)` }}>
                {badge.type}
              </span>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}

async function settle(frames = 5) {
  for (let index = 0; index < frames; index++) await profiler.nextFrame()
}

async function ready(count) {
  const started = performance.now()
  while (performance.now() - started < 30000) {
    const error = document.querySelector('[role="alert"]')
    if (error) throw new Error(error.textContent)
    const widgets = [...document.querySelectorAll('[data-widget-id]')]
    if (
      widgets.length === count &&
      widgets.every((widget) => widget.style.visibility === 'visible') &&
      document.querySelector('.moveable-control.moveable-se')
    ) {
      await settle()
      return
    }
    await profiler.nextFrame()
  }
  throw new Error('Timed out waiting for all widgets and group handles to render')
}

function mouse(target, type, x, y) {
  return profiler.measure('input.dispatch', () =>
    target.dispatchEvent(
      new MouseEvent(type, { bubbles: true, cancelable: true, view: window, clientX: x, clientY: y, button: 0, buttons: type === 'mouseup' ? 0 : 1 }),
    ),
  )
}

function presentationProbe(count, trigger) {
  const scene = document.querySelector('[data-testid="overlay-scene"]')
  return new Promise((resolve, reject) => {
    const changed = new Set()
    const started = performance.now()
    const timeout = setTimeout(() => {
      scene.removeEventListener('wasm-widget-render', rendered)
      reject(new Error(`Gesture presented ${changed.size}/${count} widgets; refusing to report an incomplete group`))
    }, 5000)
    function rendered(event) {
      changed.add(event.target.dataset.widgetId)
      if (changed.size !== count) return
      clearTimeout(timeout)
      scene.removeEventListener('wasm-widget-render', rendered)
      queueMicrotask(() => resolve(performance.now() - started))
    }
    scene.addEventListener('wasm-widget-render', rendered)
    trigger()
  })
}

async function gesture(operation, frames, burst, count, measured) {
  const target = document.querySelector(operation === 'drag' ? '[data-widget-id]' : '.moveable-control.moveable-se')
  const bounds = target.getBoundingClientRect()
  const groupBounds = currentEditor.moveableProps.moveableRef.current.getRect()
  const x = bounds.left + bounds.width / 2
  const y = bounds.top + bounds.height / 2
  const beforeStarts = gestures[operation]
  const beforeCommits = gestures.commits
  mouse(target, 'mousedown', x, y)
  await profiler.nextFrame()
  const firstPresentation = await presentationProbe(count, () => mouse(window, 'mousemove', x + 12, y + 8))
  if (gestures[operation] !== beforeStarts + 1) throw new Error(`Moveable did not begin exactly one group ${operation}`)
  await settle()
  const intervals = []
  let lastX = x + 12
  let lastY = y + 8
  let previous = await profiler.nextFrame()
  if (measured) profiler.start()
  for (let index = 0; index < frames; index++) {
    const now = await profiler.nextFrame()
    intervals.push(now - previous)
    previous = now
    for (let event = 0; event < burst; event++) {
      const phase = ((index * burst + event + 1) / (frames * burst)) * Math.PI * 2
      const dx = operation === 'drag' ? 35 + Math.sin(phase) * 70 : 20 + Math.sin(phase) * 55
      const dy = operation === 'drag' ? 20 + Math.sin(phase * 2) * 40 : (dx * groupBounds.height) / groupBounds.width
      lastX = x + dx
      lastY = y + dy
      mouse(window, 'mousemove', lastX, lastY)
    }
  }
  // Include work still queued by the final input, without mixing mouseup into steady-state costs.
  await settle()
  const trace = measured ? profiler.stop() : null
  const released = performance.now()
  mouse(window, 'mouseup', lastX, lastY)
  const releaseMs = performance.now() - released
  await settle()
  if (gestures.commits !== beforeCommits + 1) throw new Error(`Expected one document commit for group ${operation}`)
  if (trace !== null && trace.counters['geometry.presentations'] === 0) throw new Error('No widgets moved during the measurement')
  return { first_presentation_ms: firstPresentation, release_ms: releaseMs, frame_intervals_ms: intervals, ...trace }
}

/** @param {object} input Canonical fixture, activity, operation, and validated frame protocol. @returns {Promise<object>} Raw frame and stage samples. */
export async function run({ fixture, activity, operation, frames, warmup, burst, count }) {
  const module = await loadWidgetModule()
  profiler.install(module)
  const ids = buildConfigWidgets(fixture.config).map((widget) => widget.id)
  useStore.setState({
    config: fixture.config,
    globalDefaults: fixture.globals,
    parsedActivity: { ...activity, preview_activity_id: 'interaction-benchmark' },
    selectedSecond: 60,
    fallbackDurationSeconds: activity.trim_end_seconds,
    selectedWidgetId: ids.at(-1),
    selectedWidgetIds: ids,
  })
  const root = createRoot(document.getElementById('root'))
  const presented = () => profiler.count('geometry.presentations')
  document.addEventListener('wasm-widget-render', presented)
  try {
    root.render(
      <Profiler id="editor" onRender={(_id, _phase, duration) => profiler.record('react.render', duration)}>
        <BenchmarkEditor />
      </Profiler>,
    )
    await ready(count)
    if (warmup > 0) await gesture(operation, warmup, burst, count, false)
    const result = await gesture(operation, frames, burst, count, true)
    if (currentEditor.widgets.length !== count) throw new Error('The selected workload changed size')
    return { id: `${fixture.workload}-${operation}`, widgets: count, inputs: frames * burst, ...result }
  } finally {
    root.unmount()
    document.removeEventListener('wasm-widget-render', presented)
    profiler.dispose()
  }
}

window.runInteractionBenchmark = run

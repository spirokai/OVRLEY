import { useEffect, useRef, useState } from 'react'
import { createWasmRenderer } from '../utils/wasmRenderer'

/** @returns {object} Diagnostic canvas lifecycle and timing summary. */
export function useWasmPreviewDebug() {
  const canvasRef = useRef(null)
  const [status, setStatus] = useState('Loading Rust renderer…')
  const [timing, setTiming] = useState(null)

  useEffect(() => {
    let cancelled = false
    let animation
    let renderer
    let firstTime
    let previousFrame = -1
    let samples = 0
    let total = { draw: 0, copy: 0, present: 0, total: 0 }

    function tick(time) {
      firstTime ??= time
      const frame = Math.floor(((time - firstTime) * 60) / 1000)
      if (frame !== previousFrame) {
        try {
          const sample = renderer.render(frame)
          for (const key of Object.keys(total)) total[key] += sample[key]
          samples += 1
          previousFrame = frame
          if (samples === 120) {
            setTiming(Object.fromEntries(Object.entries(total).map(([key, value]) => [key, (value / samples).toFixed(2)])))
            samples = 0
            total = { draw: 0, copy: 0, present: 0, total: 0 }
          }
        } catch (error) {
          setStatus(error.message)
          renderer.dispose()
          return
        }
      }
      animation = requestAnimationFrame(tick)
    }

    createWasmRenderer(canvasRef.current)
      .then((instance) => {
        if (cancelled) {
          instance.dispose()
          return
        }
        renderer = instance
        setStatus('Running · Skia software raster · 60 fps target')
        animation = requestAnimationFrame(tick)
      })
      .catch((error) => {
        if (!cancelled) setStatus(error.message)
      })

    return () => {
      cancelled = true
      cancelAnimationFrame(animation)
      renderer?.dispose()
    }
  }, [])

  return { canvasRef, status, timing }
}

import { useEffect } from 'react'

/**
 * Linux-only cover for a WebKitGTK video bug.
 *
 * While a paused `<video>` seeks, WebKitGTK's video layer holds no frame. If nothing else on the page
 * changes, the compositor does not redraw and the old picture stays. But any repaint during the seek
 * (the timeline playhead, the widget overlay, anything) composites the empty layer, so scrubbing flashes
 * black on every step. Observed with WebKitGTK 2.54.2, NVIDIA RTX 4070 (driver 615, NVDEC), KDE Wayland.
 * Isolated by scrubbing from the console: seeks alone did not flash, page repaints alone did not flash,
 * seeks plus an unrelated element animating every frame did. None of the WEBKIT_GST_* or compositor
 * environment variables helped. WebView2 and WKWebView keep the last frame during a seek.
 *
 * While the video is paused, a canvas copy of its last landed frame covers it. Remove this once
 * WebKitGTK keeps the frame across seeks.
 *
 * @param {object} options Video and canvas refs, whether the cover is needed, and the mounted source.
 * @param {{ current: HTMLVideoElement|null }} options.videoRef Preview video.
 * @param {{ current: HTMLCanvasElement|null }} options.canvasRef Cover canvas.
 * @param {boolean} options.enabled Whether the platform blanks seeking videos.
 * @param {string|null} options.videoSrc Mounted video source; a new source remounts both elements.
 */
export function useVideoFreezeFrame({ videoRef, canvasRef, enabled, videoSrc }) {
  useEffect(() => {
    const video = videoRef.current
    const canvas = canvasRef.current
    if (!enabled || !video || !canvas) return undefined

    let capture = 0
    const hide = () => {
      capture += 1
      canvas.style.visibility = 'hidden'
    }
    const show = async () => {
      if (!video.paused || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) return
      const id = ++capture
      // Copy at displayed resolution; the 4K source frame is not needed for a cover.
      const scale = Math.min(1, Math.max(video.clientWidth / video.videoWidth, video.clientHeight / video.videoHeight) * window.devicePixelRatio)
      const bitmap = await createImageBitmap(video, {
        resizeWidth: Math.round(video.videoWidth * scale),
        resizeHeight: Math.round(video.videoHeight * scale),
      })
      if (id !== capture) {
        bitmap.close()
        return
      }
      canvas.width = bitmap.width
      canvas.height = bitmap.height
      canvas.getContext('bitmaprenderer').transferFromImageBitmap(bitmap)
      canvas.style.visibility = 'visible'
    }

    const events = { seeked: show, pause: show, loadeddata: show, play: hide, emptied: hide }
    for (const [name, handler] of Object.entries(events)) video.addEventListener(name, handler)
    show()
    return () => {
      for (const [name, handler] of Object.entries(events)) video.removeEventListener(name, handler)
      hide()
    }
  }, [canvasRef, enabled, videoRef, videoSrc])
}

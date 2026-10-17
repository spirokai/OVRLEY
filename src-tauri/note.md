Without the filter below (opacity=1) on video any wasm widgets with transparency do not blend with the video correctly somehow looking transparent/washed out at canvas zoom <100%. The filter however skyrockets the GPU usage from 20% to 50%.

Edited app\src\features\video-preview\components\VideoPreviewSurface.jsx (+0 -3)

- // An identity filter keeps video in a browser render pass. DirectComposition
- // video underlays can wash out translucent widgets and editor chrome when scaled.
  return (
  ⋮
  className={cn('pointer-events-none absolute inset-0 h-full w-full object-cover', isOutOfRange ? 'opacity-20' : 'opacity-100')}
-          style={{ filter: 'opacity(1)' }}
           preload="auto"

DOES NOT WORK - still washed out:
canvas.getContext('2d', { willReadFrequently: true })

DOES NOT WORK - performance is shit:
video.style.opacity=mode==='video-opacity'?'0.9999'

This is clearly some blending/compositing issue, I jsut do not understand why it happens on canvas zoom that zooms the entire assembly of video+widgets, their relative dimensions remain the same. 4 identical 50%-transparent colors drawn as HTML, SVG, Canvas and CPU Canvas, with no filters - all washed out too - so not specific to skia/canvas

The only thing that fixes it is - --disable_direct_composition_video_overlays=1 flag. Is there a different way? Rearranging DOM/HTML layers?

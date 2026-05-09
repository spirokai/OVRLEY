We need to wire in widget update rate.

- wire in widget update rate by defining container fps based on layout fps, not actual rendered fps
- The widget update rate means we are skipping the rendering of every n-th frame. That means the target framerate should probably be devisible by n. That will ensure we are always rendering the real values (recorded every full second) and culling only the interpolate values - do I understand this correctly?
- Assuming previous point is correct - we can provide update rates for predefined fps - 24: 2,4,8; 30: 2,5,10; 60: 2,5,10 - but we also must be able to derive update rates for custom fps somehow - we need to find 3 integers that cleanly devide the custom fps value and span sufficient range so culling makes sense.
- FPS and update rate then must be passed to renderer so it knows why interpolated values to skip and how to define the container
- Confirm if the scope of this is clear. Ask questions if there is any ambiguity at all, do NOT guess.

- Y mixed contents of widgets in the templates

-height of metric widget selection box does not correspond to the real height. Is that just stale state from font change?

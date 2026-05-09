- wire in widget update rate by defining container fps based on layout fps, not actual rendered fps
- The widget update rate means we are skipping the rendering of every n-th frame. That means the target framerate should probably be devisible by n. That will ensure we are always rendering the real values (recorded every full second) and culling only the interpolate values - do I understand this correctly?
- Assuming previous point is correct - we can provide update rates for predefined fps - 24: 2,4,8; 30: 2,5,10; 60: 2,5,10 - but we also must be able to derive update rates for custom fps somehow - we need to find 3 integers that cleanly devide the custom fps value and span sufficient range so culling makes sense.
- FPS and update rate then must be passed to renderer so it knows why interpolated values to skip and how to define the container

-height of metric widget selection box does not correspond to the real height. Is that just stale state from font change?

-border thickness, border color, shadow color, shadow strength and shadow distance are supposed to be layout-wide settings. Currently they are scoped inside the json templates per widget. Move this into the "scene" key of templates. Make sure both frontend and skia-backend read these paremeters from scene, not from individual widget parts. If scene does not provide them, assume they values are 0. Modify templates/new_template.json to include these parameters in the scene - this one is used as the testing template.

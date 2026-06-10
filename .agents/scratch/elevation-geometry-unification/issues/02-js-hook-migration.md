Status: ready-for-agent

# 02 — JS Hook Migration to Consume Rust Geometry

## Parent

[Elevation Geometry Unification PRD](../PRD.md)

## What to build

Rewrite the `useElevationPreviewGeometry` hook to call the new `backend_build_elevation_geometry` Rust command instead of computing geometry locally in JavaScript. Delete the entire duplicated JS geometry pipeline (`elevationGeometry.js`). Keep the cheap per-frame operations (marker interpolation, completed polyline filtering, SVG path materialization) in JS — these must run at 30fps with zero IPC latency.

## Acceptance criteria

- [ ] `useElevationPreviewGeometry` calls `buildElevationGeometry()` from `backend.js` and stores the result
- [ ] The hook passes Rust-provided `points` and `progressValues` to local `getPointAtMetricProgress()` for marker placement
- [ ] The hook passes Rust-provided `points` and `progressValues` to local `buildElevationCompletedPoints()` for completed polyline
- [ ] The hook uses local `getInterpolatedSeriesValue()` for the elevation label value (reads from raw activity series)
- [ ] The hook materializes SVG paths via local `pointsToSvg()` and `areaToSvg()`
- [ ] `elevationGeometry.js` is deleted entirely
- [ ] `buildScopedElevationSeries()` is removed from `exportRange.js`
- [ ] Elevation geometry constants removed from `overlayEditorConstants.js` (if no longer referenced)
- [ ] `ElevationRenderer.jsx` renders correctly with the new hook output (no changes needed to the renderer itself)
- [ ] The preview updates responsively when user adjusts y_scale, target_density, or simplify_tolerance_px sliders
- [ ] The preview updates when export range changes
- [ ] `pnpm test` passes
- [ ] `pnpm lint` passes

## Implementation notes

### Hook rewrite outline

The hook currently calls 7 sequential computation steps. After migration, it becomes:

```js
function useElevationPreviewGeometry({ activity, data, exportRange, previewSecond, style }) {
  const [geometry, setGeometry] = useState(null)

  // Call Rust when parameters change
  useEffect(() => {
    if (!activity || !data) return
    buildElevationGeometry(/* config from data + exportRange */, activity)
      .then(setGeometry)
  }, [activity, data, exportRange, style.width, style.height])

  if (!geometry) return null

  // Local per-frame computation (cheap, zero-latency)
  const progress01 = getWindowProgressAtTime(/* ... */)
  const markerPoint = getPointAtMetricProgress(geometry.points, geometry.progressValues, progress01)
  const elevationValue = getInterpolatedSeriesValue(/* raw activity series */)
  const completedPoints = buildElevationCompletedPoints(geometry.points, geometry.progressValues, progress01, markerPoint)

  // SVG materialization
  return {
    markerPoint,
    elevationValue,
    remainingSvgPoints: pointsToSvg(geometry.points),
    completedSvgPoints: pointsToSvg(completedPoints),
    // ... area paths
  }
}
```

### Config serialization

The hook needs to build a config object that matches `ValidatedRenderConfig` shape for the `elevation_plot` + `scene` fields. This is a subset of the full render config — the command only needs the elevation plot config and scene config, not the entire template.

### Debouncing

The Rust command is fast (sub-millisecond for typical activities), but the IPC round-trip adds ~1-5ms. For slider drag events, add a 50ms debounce on the geometry rebuild. The existing `useMemo` / `useEffect` dependency tracking handles this naturally — React batches state updates.

### What stays unchanged

- `geometryUtils.js` — `getPointAtMetricProgress`, `findPointAtProgress`, `getPointAtProgress`, `pointsToSvg`, `areaToSvg`
- `svgPreviewUtils.js` — `buildElevationCompletedPoints` (or inline it in the hook)
- `ElevationRenderer.jsx` — pure SVG renderer, consumes the same output shape
- `useElevationPreviewStyle.js` — presentation model
- `overlayEditorUtils.js` — `getInterpolatedSeriesValue`, `interpolateNumericSeries`

### Files to modify

| File | Change |
|------|--------|
| `app/src/features/widget-preview/hooks/useElevationPreviewGeometry.js` | Rewrite to call Rust command, keep local interpolation |
| `app/src/features/widget-preview/utils/elevationGeometry.js` | **Delete entirely** |
| `app/src/features/overlay-editor/utils/exportRange.js` | Remove `buildScopedElevationSeries()` |
| `app/src/features/overlay-editor/data/overlayEditorConstants.js` | Remove elevation geometry constants |

## Blocked by

[#01 — Rust Geometry Command + IPC Contract](01-rust-geometry-command.md)

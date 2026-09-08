# Sync signals findings — manual video/data sync

## Summary

For cycling and action-sport activities, the four baseline signals are **GPS position, speed, elevation, and heading**. In OVRLEY these are guaranteed to a useful degree because the Rust finalizer derives speed from GPS distance/time, derives heading from a 2 m GPS baseline, and derives elevation from GPS or barometric altitude when available.

The question is which *events* built from those (and optional) signals are easiest for a non-expert to point to in both the video and the data timeline. The clear winners are **movement state changes**: the first moment the rider starts moving, and any full stop. They are unambiguous on a speed graph, obvious on a map route, and trivial to recognise in video.

Heading changes, speed peaks, and elevation crests/dips are the next tier. They require a little more interpretation but are still strong when used in combination.

Optional sensor events — heart-rate spikes, power spikes, and manual lap buttons — are helpful only when the recording actually contains them. Map landmarks are attractive but depend on external map data and are therefore a secondary convenience, not a primary sync anchor.

## Signal/event evaluation

| Signal / event | Typical file availability | Obvious on speed/elevation graph | Obvious on map route | Obvious in video | Caveats |
| --- | --- | --- | --- | --- | --- |
| **Start / stop of movement** (speed crosses zero) | Very high — speed is always present, either direct or derived from GPS distance/time. | Very high — sharp vertical edge from/to zero. | High — the moving dot on the route starts or freezes. | Very high — rider pushes off, camera starts rolling, or comes to a halt. | Initial GPS drift can create a tiny non-zero speed before real motion. Camera may be started after the rider is already moving. |
| **Stops / pauses** (stationary periods, timer events) | High — FIT files contain explicit `timer` start/stop events; GPX files expose pauses as speed=0 gaps or missing samples. | Very high — flat zero-speed segment, often with a preceding braking spike. | High — points cluster or the route trace has no forward progress. | Very high — traffic lights, junctions, dismounting, putting the bike down. | Auto-pause can drop records entirely; OVRLEY gap-fills these as synthetic idle samples, so the event may be inferred rather than raw. Slow walking can look like a stop. |
| **Sharp turns / heading changes** | High — heading is direct in FIT when recorded, otherwise derived from GPS over a 2 m baseline. | Low on speed alone; better if brake/speed dip is shown. | Very high — the route trace bends sharply. | High — handlebar turn, road curvature, body lean. | Heading is noisy at low speed and in tight switchbacks. Circular EMA smoothing hides jitter but can shift the apparent peak slightly. |
| **High-speed moments / descents** | High — speed is always available. | High — prominent peak on the speed graph. | Moderate — route looks straight/downhill only if terrain is visible. | High — fast scenery motion, wind noise, suspension compression. | GPS-derived speed can lag and can spike on poor signals. Not all speed peaks are visually distinctive (e.g. fast flat section). |
| **Elevation crests / dips** | High — elevation is always present (GPS or barometric). | Moderate on an elevation graph; very high for large climbs/descents. | Low on a 2D route map unless the path is colour-coded by elevation or gradient. | Moderate/high — summit views, change in horizon, descent posture. | GPS-only elevation is noisy; barometric altitude is much better. Small rollers may not be visually obvious. |
| **Manual lap / button events** | Low–moderate — FIT supports `lap` and `event` messages, but OVRLEY's current FIT/GPX parsers do not extract them. GPX has no standard lap marker. | Low — a vertical marker line is useful only if the user remembers pressing the button. | Low — usually just a point on the route. | Low — button press is rarely visible. | Requires parser changes to expose FIT event/lap records. Users often do not press lap during a casual ride. |
| **Heart-rate spikes** | Low — requires an HR sensor and for the app/device to record it. | Moderate — needs a separate HR graph or combined view. | None — HR has no spatial signature. | Moderate — visible effort, standing, heavy breathing, but delayed by HR response. | HR lags effort; dropouts and spikes from strap movement are common. |
| **Power spikes** | Low — requires a power meter. | Moderate — needs a power graph. | None. | Low/moderate — effort is visible but power spikes are not. | Power data is noisy and only available to a minority of users. |
| **Map landmarks** (intersections, bridges, tunnels) | Low without external data — GPS gives coordinates, not semantics. | Low. | High if a map overlay or route-matching service is used. | High when the landmark is visible. | Requires map-matching or reverse geocoding; tunnels/bridges can break GPS. Adds external dependency and privacy concerns. |

## Recommended priority order for a sync-doctor screen

1. **Start / stop of movement** — the strongest anchor. Almost always available, unambiguous on the speed graph, and trivial to identify in the video.
2. **Stops / pauses** — excellent for confirming absolute time alignment. The flat zero-speed region is hard to miss, and the matching video moment (traffic light, junction, standing still) is usually obvious.
3. **Sharp turns / heading changes** — best used with the route map visible. The combination of a visible bend in the route and the rider turning in the video is very convincing.
4. **High-speed moments / descents** — large speed peaks are easy to scrub to and match to fast-motion footage; use as a secondary anchor after movement/turns.
5. **Elevation crests / dips** — useful for long climbs and descents where speed alone is ambiguous; show on an elevation graph alongside speed.
6. **Manual lap / button events** — high precision when present, but availability is limited; show as optional markers if the parser is extended to read FIT event/lap messages.
7. **Heart-rate spikes** — useful bonus for riders with HR sensors, but should not be a primary sync signal.
8. **Power spikes** — niche; only show if power data is present.
9. **Map landmarks** — nice-to-have enrichment via map overlay, but do not rely on it for core sync.

## Rationale

The priority rewards signals that are **universal**, **visually unambiguous**, and **cheap to compute**. Movement state changes score highest on all three counts. Heading changes and speed peaks are also strong but need context (map view, descent cues). Elevation features are reliable but less visible in the video. Optional sensors and external map data are pushed down because they either require hardware most users do not have, or require new dependencies and parser work.

## Implementation notes for OVRLEY

- The current FIT/GPX/SRT parsers do not expose FIT `event`/`lap` messages. To support manual lap/button events, the FIT parser should read `events` and `laps` and emit `lap_number`/`lap_markers` into the `RawActivity`.
- The Rust finalizer already produces `speed`, `heading`, `elevation`, `gradient`, `distance`, and `course` series, so movement detection, stop detection, turn detection, and crest/dip detection can be implemented as pure post-processing on the frontend.
- Stop detection should account for OVRLEY's synthetic idle gap-fill: a detected pause may be an inferred flatline rather than a raw zero-speed record.
- Heading-change detection should ignore samples below a low-speed threshold (e.g. < 3 km/h) to avoid GPS jitter.
- A sync-doctor UI should show speed and elevation graphs plus a route map, with clickable detected events that snap the data playhead and video playhead together.

## References

- Garmin FIT SDK activity-file docs: event and lap messages indicate timer start/stop and manual/auto laps. <https://developer.garmin.com/fit/file-types/activity/>
- Garmin FIT cookbook on pauses/encoding: timer events are the canonical way to encode stops. <https://developer.garmin.com/fit/cookbook/encoding-activity-files/>
- Stack Overflow discussion of FIT pause encoding. <https://stackoverflow.com/questions/46513266/how-to-encode-pauses-in-garmin-fit-files>
- HERE route-matching docs: consumer GPS accuracy is typically 0.5–40 m and heading/speed become unreliable below ~10 km/h. <https://docs.here.com/routing/docs/trace-files>
- GPX Overlay manual-sync tutorial: speed and elevation are the primary cues users already look for. <https://gpxoverlay.com/tutorials/syncing-gpx-overlay-with-video>

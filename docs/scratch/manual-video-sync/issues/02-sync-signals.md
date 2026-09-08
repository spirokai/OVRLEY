# Which activity signals and events are most usable for manual sync?

Status: closed
Labels: wayfinder:research

## Question

Given that OVRLEY activities almost always have GPS, speed, elevation, and heading, which signals or derived events are easiest for a layman to correlate with a video moment? Consider: stops/pauses, high-speed moments, sharp turns, elevation crests/dips, start/finish lines, manual lap buttons, etc. For each, note how reliably it appears in typical `.fit`/`.gpx` files and how visually obvious it is on both a graph and a map route. Recommend the priority order of signals to show in the sync doctor.

## Blocking

None.

## Comments

## Resolution

Movement state changes are the most usable sync anchors for a layman: the moment the rider starts or stops moving is almost always present, shows up as a clear edge on the speed graph, and is trivial to spot in the video. Stops/pauses are almost as strong, especially when OVRLEY's idle gap-fill has kept the zero-speed interval visible.

Heading changes and high-speed moments are the next best signals, but they work best when the sync-doctor shows a route map alongside the speed graph. Elevation crests and dips are reliable but less visually obvious in the video. Manual lap/button events are precise but not currently extracted from FIT files and are rarely recorded in casual rides; heart-rate and power spikes are niche because they require extra sensors.

Recommended priority for the sync-doctor UI:

1. Start / stop of movement
2. Stops / pauses
3. Sharp turns / heading changes
4. High-speed moments / descents
5. Elevation crests / dips
6. Manual lap / button events
7. Heart-rate spikes
8. Power spikes
9. Map landmarks

Full evaluation and rationale are in [02-sync-signals-findings.md](../02-sync-signals-findings.md).

# NEW WIDGET TEMPLATE

Spec for a new widget or display type. Fill this in — `adding-new-widgets.md` turns it into
implementation. Leave a section empty when its activation line is off.

## TRIAGE

- **new_metric:** no — Does it show data OVRLEY doesn't parse yet (e.g. added torque)?
- **display_type:** no — Is it a new look for data that already exists (e.g. a compass for heading)?
- **independent:** no — Is it a standalone element with its own visuals (e.g. route map, elevation profile)?

## GENERAL INFORMATION

- **name:** unique id, lowercase_with_underscores (e.g. heading_compass)
- **data:** which activity data it displays (existing field, or a new one if new_metric)
- **image reference:** mockup/screenshot

## METRIC CONTRACT (new_metric: yes; leave empty if data already exists)

- **default unit:** the unit shown by default (e.g. km/h, %, W)
- **other units:** other units the user may switch to (value / label / render label)
- **number format:** how the value is displayed — whole number, one decimal, etc.
  (reuse an existing style — speed, temperature, pace, integer — or describe a new one)
- **between recorded samples:** smooth (linear) / hold last value / leave as recorded
- **unit picker:** can the user switch units, or is it fixed?
- **category:** drawer grouping (general, cycling, running, motosports, camera, other)

## CONFIGURATION FIELDS

Every setting the user can adjust in the editor. One line each:
`name (lowercase_with_underscores) — control — default — allowed values`.
Skip generic ones (position, size, opacity).

- **slider:** indicator_size — 100 — 10..400
- **switch:** show_cardinal_labels — true
- **select:** rotation_mode — indicator | labels
- **color:** indicator_color — #ffffff
- **text/font:** font — scene default

## BEHAVIOR

How it looks and reacts to data — only things a viewer would notice
(rotation modes, clamping limits, what moves vs stays fixed, …).

## GESTURES

How the user manipulates it on the canvas — only things that differ from a normal widget.

- **resize:** which handles resize it (corners, edges, only via size fields, not resizable)
- **proportions:** free, or locked (e.g. a circle stays circular)
- **rotation:** can the user rotate it with a handle?
- **layering:** follows widget order, or always in its own layer when overlapping (e.g. map-like)
- **text:** does it contain user-editable text (font choice needed)?

## CACHING

Which visual parts never change for the whole video (background, track, fixed labels)
vs which animate with the data (indicator, value). Cull once, redraw the rest per frame.

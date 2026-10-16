# NEW WIDGET SPEC — heading_compass

Spec for a new widget or display type for OVRLEY. `adding-new-widgets.md` turns it into
implementation. Leave a section empty when its activation line is off.

## TRIAGE

- **new_metric:** no
- **display_type:** yes — new look for existing heading data
- **independent:** no

## GENERAL INFORMATION

- **name:** heading_compass
- **data:** heading
- **image reference:** true

## METRIC CONTRACT

(empty — display type for existing data)

## CONFIGURATION FIELDS

Grouped by the element they configure, or a general section. Position, size, transparency, and
display_type configuration are standard. One line each:
`name (lowercase_with_underscores) — control — default — allowed values`.

- **slider:** indicator_size, indicator_corner_radius, cardinal_font_size, value_font_size,
  track_thickness, value_horizontal_offset, value_vertical_offset, track_corner_radius
- **switch:** display_cardinal_labels, display_value
- **select:** font_family
- **color:** indicator_color, track_fill, cardinal_label_color, value_color, unit_color
- **toggle group:** rotation_mode — indicator | compass
- **numeric input:**
- **text input:**
- **special:**

## BEHAVIOR

- indicator is triangle with cutout (like a compass needle)
- corner radius must be limited by size of the indicator; in relative manner
- global shadow affects the indicator, the cardinal labels, unit, value, and the track
- track width should increase outwards to prevent squishing the labels and indicator; track arc is
  fixed and non-configurable
- track does not rotate with the heading value, it is fixed in place
- two rotational modes exist:
  - either the indicator rotates with the heading value,
  - the cardinal labels rotate with the heading value, indicator remains fixed pointing top
- small fade out area must exist at the edge of the track to prevent labels from clashing with the
  value as they rotate

## GESTURES

- **resize:** yes
- **proportions:** locked
- **rotation:** no
- **layering:** follows widget order
- **text:** yes — cardinal labels and value use a chosen font

## CACHING

- track should be cached in static layer
- either cardinal labels or indicator should be cached in static layer depending on the rotation mode

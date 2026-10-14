# NEW WIDGET TEMPLATE

This documents serves as a spec for creation of a new widget or display type for OVRLEY. It describes the general information, configuration fields, behavior, and caching requirements for the widget and provides a reference image.

## GENERAL INFORMATION

- **name:** heading_compass
- **display_type or independent:** display type for metric widgets
- **metric/activity column:** heading
- **image reference:** true

## CONFIGURATION FIELDS

Always group the inputs according to the element they configure, or place them in a general section. Standard position, width/height, or size, transparency, display_type configuration is required.

- **numeric input:**
- **text input:**
- **slider:** indicator size, indicator corner radius, cardinal font size, value font size, track thickness, value horizontal offset, value vertical offset, track corner radius
- **switch:** display cardinal labels, display value
- **dropdown:** font family
- **color input:** indicator color, track fill, cardinal label color, value color, unit color
- **toggle group:** indicator vs cardinal label rotation
- **special:**

## BEHAVIOR

- indicator is triangle with cutout (like a compass needle)
- corner radius must be limited by size of the indicator; in relative manner
- global shadow affects the indicator, the cardinal labels, unit, value, and the track
- track width should increase outwards to prevent squishing the labels and indicator; track arc is fixed and non-configurable
- track does not rotate with the heading value, it is fixed in place
- two rotational modes exist:
  - either the indicator rotates with the heading value,
  - the cardinal labels rotate with the heading value, indicator remains fixed pointing top
- small fade out area must exist at the edge of the track to prevent labels from clashing with the value as they rotate

## CACHING

- track should be cached in static layer
- either cardinal labels or indicator should be cached in static layer depending on the rotation mode

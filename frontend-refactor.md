# Frontend Refactor

-FINAL DELIVERABLE OF THE FRONTEND REFACTOR: Pixel-perfect state of the layout stored in Zustand ready for passing to Rust/Python backend for rendering and encoding.

# UI changes

-preserve the general styling of the frontend
-purely ReactJS with react-movable, Immer and Zustand for a reactive building of the overlay
-the overlay editor should enable dragging, resizing and snapping, with guides automatically appearing when aligning elements (do we need to display text as a vector graphic to enable resizing?)
-after the user decides to render, the overlay is translated from React into a format suitable for the backend render
-allow saving/loading templates (presumably as json-based files?)
-preserve sidebar control, but introduce two tabs: one for global settings, one with quickmenu+accordion containing widgets

# Template section

-load/save template (preserve current implementation)
-located at top of the settings tab in the sidebar

# Global settings

-Located in the settings tab of the sidebar underneath the template section
-Select aspect ratio (presets + custom dropdown; custom enables 2 inputs field in resolution)
-Select resolution controlled by aspect ratio (dropdown), input fields for custom
-Select framerate (default 30 fps)
-Select widget update rate (default 30 fps)-> slider to 1/2, 1/4, 1/8th update rate
-Export range all/custom. Custom range should support inputting both seconds from/to, and time of recording from/to
-Font for values
-Font for text
-Default color values
-Default color text
-Default color icons
-Default border color+thickness
-Default shadow strength+distance
-Default opacity
-Scale UI components
-Small reset icon in top right corner

# Quickmenu

-Located on the widgets tab in the sidebar
-Array of Icon Buttons to add different wdigets (2 rows by 5 buttons)
-For now we start with text, time&date, route map, elevation profile, speed, heart rate, cadence, power, gradient, temperature
-Clicking a button adds the default widget of that type into the editor

# General Widgets Info

-Can be moved, dragged, resized and snapped/aligned in the editor
-Are displayed as items in an accordion underneath the quickmenu (1 element expanded at time)

# Widget Customization

# Text Widget

-Font size (slider+input)
-Color (color picker)
-Text (text input)

# Speed/Heart rate/Cadence/Power Widgets

-Icon display (switch)
-Icon color (color picker)
-Icon size (slider)
-Icon offset x and y
-Font size (slider+input)
-Font color (color picker)
-Display units (switch)
-Units for speed (dropdown; kmh, mph, kn, m/s)

# Time & Date Widget

-Format (dropdown with different formats displayed as DD-MM-YYYY, 24 and 12h time formats etc)
-Include format options with only date, only time, or mixed
-Font size (slider+input)
-Icon display (switch)
-Icon color (color picker)
-Icon size (slider)
-Icon offset x and y

# Gradient/Slope Widget

-Value font size (slider+input)
-Value offset (slider)
-Value color (color picker)
-Triangle color positive/negative (2 color pickers)
-Show sign (switch)
-Display +/- (switch)
-Decimals (slider 0-2)
-Display triangle shape (switch)
-Triangle shape width (slider)

# Temperature Widget

-Icon display (switch)
-Icon color (color picker)
-Icon size (slider)
-Icon offset x and y (slider)
-Font size (slider+input)
-Font color (color picker)
-Display units (switch)
-Units C/F (switch with units inside)

# Route Map

-Line thickness (slider 0-20) - separately for completed/not completed
-Line color (color picker) - separately for completed/not completed
-Line opacity (slider 0-100) - separately for completed/not completed
-Marker size (slider 0-50)
-Marker color (color picker)
-Marker opacity (slider 0-100)
-Map rotation (0-360 degrees, number input, or some slider?)

# Elevation profile

-Line thickness (slider 0-20) - separately for completed/not completed
-Line color (color picker) - separately for completed/not completed
-Line opacity (slider 0-100) - separately for completed/not completed
-Marker size (slider 0-50)
-Marker color (color picker)
-Marker opacity (slider 0-100)
-Show elevation metric (switch)
-Show elevation imperial (switch)
-Offset for both elevation labels (x,y with respect to the marker; slider)

# Allow import both .gpx and .fit files

-Support all the fields below, some of them are for future compatibility
-Ensure the processed data are standardized for the Python/Rust backend
Latitude / Longitude
Course/heading
Altitude
Timestamp
Speed
Pace
Distance
Vertical speed
Heart rate
Cadence
Power
Left/right balance
Torque
Ground contact time
Vertical oscillation
Stride length
Temperature
Air pressure
Slope/gradient
Stroke rate (rowing)
G-Force

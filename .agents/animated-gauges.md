Animated gauges will extend the metric widgets. The user will be able to pick (using a dropdown menu), if they want 1. Metric widget (current state) 2. Linear 3. Bars 4. Arc 5. Corner. First I will describe the styling options and mechanisms they will share, then I will provide the specifics for each type.

Common features:
-A track that covers between minimum and maximum value of that metric and fills depending on the present value
-Resizable in both directions.
-Custom track thickness, track corner radius, track border thickness/color, track empty/filled color&opacity.
-Minimum/maximum label on/off, label font size, label color
-The labels, empty tracks with borders should be rendered by rust-skia into the static cached layer so they are not redrawn on each frame
-Only the filled part that changes with actual value is rendered dynamically each frame
-Skia-rust rendering in the backend MUST be implemented FIRST - this is non-negotiable - it will serve as the source of truth
-Frontend preview is implemented only afterwards and must mirror the skia-rust approach perfectly
-For any text, we are using svg text in both front and backend

Specifics: 2. When selected, only the continuous, linear bar is shown, the current "metric widget" is not visible, i.e. no current value or units are visible. Additional option - vertical/horizontal determines if the track should be oriented vertically or horizontally. 3. For the bars option, a linear array of bars separated by a gap appears. This widget has additional styling options: a) gap - the defines the gap between the bars, b) bar count - number of bars in the widget. Again, only the bar track is shown here, the current "metric widget" is not visible. Same vertical/horizontal option as in 1. 4. Arc renders a track in the shape of a circular arc of a defined angle. The angle of the arc is a styling option and the arc is symmetric along the vertical axis, i.e. 180 degree arc would be a half circle starting and ending at a horizontal line. Here, the metric widget is still shown in the centre of the arc. The position of the metric widget and its size within the arc must be customizable, i.e. x/y offset from the centre is added, and all the current metric widget options are still available for that part of the gauge. In this arrangement, the unit is positioned under the value, not next to it like in the current widget. 5. Same as arc, except the arc is always 90 degrees and the gauge can be oriented as top/bottom left/right corner. Also for this gauge, the metric widget stays visible and can be modified like described in point 4.

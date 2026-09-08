# GOAL

Let user sync video and activity manually by designating specific landmarks in video manually, scan the activity data for same type of landmarks and finding candidates with best correlation, the let user pick different candidates from the list and automatically apply the offset to the video.

## SPEC

### LOCATION

The feature lives in its own directory `src/features/video-sync`. A toolbar drawer must be registered in `src/features/toolbar`. The icon must be simple clock without the ticks, the outline of the clock should be a large circular arrow.

### LEFT TOOLBAR

- Section 1: Video sync section from @VideoDrawerContent will be at the top. Do not abstract, we will likely change its layout here a bit.
- Section 2: Landmark section with a trailing small button "clear" that will remove all landmarks. The landmarks should be listed as 1-row cards with a colored stripe on the left, colored icon. Rest of text is not color. The card also contains time-since-start of the video and a trash icon to remove the landmark. The card is clickable and will scrub the video to that landmark. The colors are: stop-red, turn-green, location-purple. The landmarks are always sorted by time-since-start of the video. We will allow maximum of 5 landmarks, see behavior.
- Section 3: Candidate section. First a column with 2 sliders - "speed sensitivty" and "turn sensitivity". Speed sensitivity determines threshold for speed in activity that will be considered as stops (default <5 kmh). Turn sensitivity is the same but for heading. This should be reasonable range, we want to consider anything between approximately 90 degree to 360 degrees per relatively short duration to be a turn. 180 degree change over 2 minutes is not considered a turn. See behavior for more details. Below this a two-column button "Landmark sync" starts the correlation (see behavior). Will display series of cards with the candidate offset and the correlation score. The cards will be sorted by correlation score, highest first. If there are no candidates found for a landmark, we will show a message "No candidates found for this landmark". We will allow maximum of 5 candidates.

### VIDEO TIMELINE AREA

- A graph area will be shown between the timeline ruler and the timeline lanes. The graph will show a plot of speed and heading (same colors as marks) over time, it's zoom and timelineviewport is controlled by the same thing that controls the timeline lanes and the ruler.
- The graph line for information purposes only, does not change and is not interactive - it is dictated by activity and only changes when activity changes. Unresolved question: What does we use to plot it? Polyline? Some plotting package?
- The graph area will show automatically detected stops and turns as vertical bands (detected+-2 seconds) in same colors as marks. THe landmarks will be displayed as vertical lines in same colors as marks with a rectangular handle and icon inside at the top - the span across the timeline lanes and ruler - same as playhead.
- The landmark tags/lines are draggable and update the landmark time in the landmark list. They do not trigger recalculation of candidates, only the "Landmark sync" button does that.
- If landmarks are outside the timeline viewport, the landmark lines/tags will be shown the sides of the timeline viewport - basically limit their display to viewport area.

### EDITOR CANVAS

- Left bottom corner of the actual video preview: column of 3 buttons: "Mark Stop", "Mark Turn", "Mark Location". Mark location button is disabled for now, but it should be visible. They should have the corresponding landmark colors. The buttons are visible but disabled when the playhead is outside video range.
- Right side of the actual video preview: Two permanent widgets - speed and map route. Marker of map route must be small and bright red, map route white with no opacity - we need good contrast. These widgets are permanent, not editable and are NOT part of the project or template. They are a diagnostic feature.
- Widgets from the project/template are not shown in this view, but they are still present in the project/template and will be shown on this screen.

### BEHAVIOR

- "speed sensitivty" and "turn sensitivity" will recalculate "turn" and "stop" events from the activity data, update the candidate list accordingly, update the correlation score, and their heighlight in the "chart" area. We will start with oncommit recalculation, if the performance is bad we will couple this to the "Landmark sync" button.
- "Landmark sync" button will start the correlation process for all landmarks, and update the candidate list accordingly. It must be disabled during the correlation process.
- Maximum 1 map landmark allowed at any time.
- Maximum of 5 landmarks total allowed at all times.
- Landmarks are saved in the project data, in "landmarks" array so they can be reproduced when project loads.
- Mark "stop" in matching always assumes the user marked the moment when they stopped moving, not a random moment during the stop (which can be several minutes long). Mark "turn" does not assume anything, turns are typically several seconds long, and the user can mark any moment during the turn. Mark "location" is any timepoints in the vieeo.
- Always show 1 candidate that is map landmark-ONLY, if map landmark exists. It should be clearly labelled and always on top of the list, it should not have a confidence score, just say "map only".
- Landmarks correlate the landmarks to detected starts/stops. The type of landmark must be respected during correlation, but they should be cross-corelate/multivariate/whatever it is call. Basically what I am saying if there is 1 turn landmark, you don't fit it independently and say turn confidence correlation is "1". You always correlate all landmarks together. The correlation/confidence score should be a statistically valid score with range 0-100 - use approporiate statistical method to calculate it. Unresolved question: What algorithm do we use to correlate the landmarks to activity data?
- During correlation assume user's landmark precision is +- 2 seconds.
- Unresolved question: how to consider map landmarks in the correlation algorithm - there is nothing to fit - user will EVENTUALLY (not part of this feature) mark a location on map and they should not be wrong. However what if user's map landmark and other landmarks disagree? How do we handle that? Do we weight the map landmark? Do we ignore it completely for fits that disagree and mark the candidate that map was ignored?
- How are "turn" and "stop" landmarks derived from the activity data? should we use thresholding method? Should we use derivation?
- Landmarks are tied to start of the video - if user drags video in the timelineLane, or changes manual offset, the landmarks must move with the video. If user drags a landmark in the timelineLane, it will change the landmark time and update the landmark list, but it will not change the video offset.
- "sync landmarks" button is disabled if there is no map landmark and less than 2 other landmarks.
- Ignore mutliple clip workflow. This is a single clip feature only.

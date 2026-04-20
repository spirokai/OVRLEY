It mostly already does that. I’m mapping your list directly onto the current parser output so the answer is precise about what’s already covered and what still requires backend work.

Yes. That is largely what the new parser is already doing in activityParserUtils.js.

The current parsed activity already includes the legacy-shaped fields you listed:

valid_attributes
course, elevation, time, speed, gradient, heartrate, cadence, power, temperature
sample_elapsed_seconds
sample_distance_progress
sample_course_points
sample_elevations
trim_start_seconds, trim_end_seconds
frame_elapsed_seconds, frame_timestamps, frame_distance_progress initialized as empty arrays
So your intuition is correct: the frontend parser can absolutely produce the legacy renderer data shape.

What you are missing is not the data shape. It is the runtime contract.

The legacy backend does not just expect a dictionary with fields. It expects an Activity-like object with behavior from activity.py, including methods and lifecycle like:

parsing/setup in the constructor
trim(...)
interpolate(...)
duration_seconds()
integer_duration_seconds()
interpolation helpers used during trimming/render flow
It also expects Python-native value types in places:

time and source_start_time as Python datetimes, not JSON strings
object attributes accessible with dot syntax, not just nested JSON
arrays already attached to the activity instance so scene.py can read them directly
So the answer is:

Yes, the new parser can and already does produce the legacy field set.
No, that alone is not enough for the current backend renderer to consume it unchanged.
The missing piece is a backend adapter that turns parsed JSON into a renderable Activity-like Python object.
In practice, the clean design is:

Frontend parser remains the source of truth for normalized activity data.
Preview/render send that parsed activity to the backend.
Backend creates something like a ParsedActivity or ActivityFromJson object that:
hydrates JSON fields onto attributes
converts time strings to Python datetimes
preserves the legacy attribute names and caches
implements trim, interpolate, and duration helpers the same way the old Activity class does
That gives you the best of both worlds:

one parser path
editable activity data before render
minimal renderer churn because scene.py can still talk to an Activity-like object
So the short answer is: yes, the parser can do the field translation, and it mostly already does. The remaining work is backend object adaptation, not more frontend normalization.

If you want, I can implement that next:

Add a Python adapter that builds a legacy-compatible activity object from parsed JSON.
Wire preview/render to send parsedActivity instead of gpxFilename.

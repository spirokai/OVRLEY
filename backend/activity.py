import logging
from bisect import bisect_left, bisect_right
from collections import defaultdict
import sys
from datetime import timedelta

from gradient import derive_gradients

print("DEBUG: constant imported", file=sys.stderr)
sys.stderr.flush()

import constant

# Lazy imports for heavy libraries
# gpxpy, numpy, and scipy are imported inside methods where needed

ATTRIBUTE_MAP = {
    "{http://www.garmin.com/xmlschemas/TrackPointExtension/v1}atemp": "temperature",
    "{http://www.garmin.com/xmlschemas/TrackPointExtension/v1}hr": "heartrate",
    "{http://www.garmin.com/xmlschemas/TrackPointExtension/v1}cad": "cadence",
    "{http://www.garmin.com/xmlschemas/PowerExtension/v1}PowerInWatts": "power",
    "power": "power",
    "{http://www.garmin.com/xmlschemas/GpxExtensions/v3}Temperature": "temperature",
}
PARENT_TAGS = {
    "{http://www.garmin.com/xmlschemas/TrackPointExtension/v1}TrackPointExtension"
}


class Activity:
    def __init__(self, gpx_filename):
        try:
            # Expect a valid path; do not prefix with './' (breaks absolute paths)
            logging.info(f"Activity: Opening GPX file: {gpx_filename}")
            import gpxpy

            with open(gpx_filename, "r") as f:
                self.gpx = gpxpy.parse(f)
            logging.info(
                f"Activity: GPX parsed successfully, tracks: {len(self.gpx.tracks)}"
            )
            self.source_start_time = None
            self.sample_elapsed_seconds = []
            self.sample_distance_progress = []
            self.frame_elapsed_seconds = []
            self.frame_timestamps = []
            self.frame_distance_progress = []
            self.trim_start_seconds = 0.0
            self.trim_end_seconds = 0.0
            self.sample_course_points = []
            self.sample_elevations = []
            self.set_valid_attributes()
            self.parse_data()
        except FileNotFoundError:
            logging.error(f"Activity: GPX file not found: {gpx_filename}")
            raise
        except Exception as e:
            logging.error(f"Activity __init__ error: {type(e).__name__}: {e}")
            import traceback

            traceback.print_exc()
            raise

    def set_valid_attributes(self):
        present_attributes = set()
        attribute_map = ATTRIBUTE_MAP
        tag_map = {}
        track_points = self.gpx.tracks[0].segments[0].points
        # not all extensions are present in all track points
        # TODO this needs work - probably don't need to set attributes - should be able to parse data in single pass
        track_points = [
            track_points[0],
            track_points[len(track_points) // 2],
            track_points[-1],
        ]
        for track_point in track_points:
            if track_point.latitude and track_point.longitude:
                present_attributes.update({constant.ATTR_COURSE, constant.ATTR_SPEED})
            if track_point.time:
                present_attributes.add(constant.ATTR_TIME)
            if track_point.elevation:
                present_attributes.add(constant.ATTR_ELEVATION)

            for ii, extension in enumerate(track_point.extensions):
                if extension.tag in attribute_map.keys():
                    present_attributes.add(attribute_map[extension.tag])
                    tag_map[attribute_map[extension.tag]] = ((ii, extension.tag),)
                for jj, child_extension in enumerate(extension):
                    if child_extension.tag in attribute_map.keys():
                        present_attributes.add(attribute_map[child_extension.tag])
                        tag_map[attribute_map[child_extension.tag]] = (
                            (ii, extension.tag),
                            (jj, child_extension.tag),
                        )

        if {constant.ATTR_COURSE, constant.ATTR_ELEVATION}.issubset(present_attributes):
            present_attributes.add(constant.ATTR_GRADIENT)

        self.valid_attributes = list(present_attributes)
        self.tag_map = tag_map

    def parse_data(self):
        def smooth_series(values, window_length, polyorder):
            from scipy.signal import savgol_filter

            if len(values) < 3:
                return list(values)

            usable_window = min(window_length, len(values))
            if usable_window % 2 == 0:
                usable_window -= 1
            minimum_window = polyorder + 2
            if minimum_window % 2 == 0:
                minimum_window += 1
            if usable_window < minimum_window:
                return list(values)

            return savgol_filter(
                values,
                window_length=usable_window,
                polyorder=polyorder,
            ).tolist()

        def parse_attribute(tag_map, trackpoint):
            extension = None
            for index, tag in tag_map:
                extensions = extension if extension else trackpoint.extensions
                if index < len(extensions) and tag == extensions[index].tag:
                    extension = extensions[index]
                else:
                    for e in extensions:
                        if e.tag == tag:
                            extension = e
                            break
                    if extension is None:
                        if index < len(extensions):
                            pass
                            # print("wtf 1")
                        else:
                            pass
                            # print("wtf 2")
                        return 0.0

            raw_value = extension.text if extension is not None else None
            if raw_value is None:
                return 0.0

            text_value = raw_value.strip()
            if not text_value:
                return 0.0

            try:
                return float(text_value)
            except (TypeError, ValueError):
                logging.warning(
                    "Activity: Non-numeric extension value %r for tags %s; defaulting to 0.0",
                    raw_value,
                    tag_map,
                )
                return 0.0

        data = defaultdict(list)
        track_segment = self.gpx.tracks[0].segments[0]
        previous_point = None
        cumulative_distance_m = 0.0
        sample_distances_m = []
        for ii, point in enumerate(track_segment.points):
            if previous_point is None:
                sample_distances_m.append(0.0)
            else:
                distance_m = point.distance_2d(previous_point) or 0.0
                cumulative_distance_m += distance_m
                sample_distances_m.append(cumulative_distance_m)
            for attribute in self.valid_attributes:
                match attribute:
                    case constant.ATTR_COURSE:
                        data[attribute].append((point.latitude, point.longitude))
                    case constant.ATTR_ELEVATION:
                        data[attribute].append(point.elevation)
                    case constant.ATTR_TIME:
                        data[attribute].append(point.time)
                    case constant.ATTR_SPEED:
                        data[attribute].append(track_segment.get_speed(ii))
                        # data[attribute].append(point.speed) - for some reason, point.speed isn't interpreted correctly (always None). maybe try other gpx files to see if it works in other cases?
                    case (
                        constant.ATTR_CADENCE
                        | constant.ATTR_HEARTRATE
                        | constant.ATTR_POWER
                        | constant.ATTR_TEMPERATURE
                    ):
                        data[attribute].append(
                            parse_attribute(self.tag_map[attribute], point)
                        )
            previous_point = point

        for attribute in self.valid_attributes:
            if attribute == constant.ATTR_ELEVATION:
                data[attribute] = smooth_series(
                    data[attribute],
                    window_length=11,
                    polyorder=3,
                )
            setattr(self, attribute, data[attribute])

        if constant.ATTR_GRADIENT in self.valid_attributes:
            gradient_elevations = smooth_series(
                data.get(constant.ATTR_ELEVATION, []),
                window_length=7,
                polyorder=2,
            )
            data[constant.ATTR_GRADIENT] = derive_gradients(
                gradient_elevations,
                sample_distances_m,
            )
            setattr(self, constant.ATTR_GRADIENT, data[constant.ATTR_GRADIENT])

        timestamps = getattr(self, constant.ATTR_TIME, [])
        self.source_start_time = timestamps[0] if timestamps else None
        self.sample_elapsed_seconds = self.build_sample_elapsed_seconds(timestamps)
        self.trim_start_seconds = 0.0
        if self.sample_elapsed_seconds:
            self.trim_end_seconds = self.sample_elapsed_seconds[-1]
        elif self.valid_attributes:
            self.trim_end_seconds = float(
                len(getattr(self, self.valid_attributes[0], []))
            )
        else:
            self.trim_end_seconds = 0.0

        if sample_distances_m:
            total_distance_m = sample_distances_m[-1]
            if total_distance_m > 0:
                self.sample_distance_progress = [
                    distance_m / total_distance_m for distance_m in sample_distances_m
                ]
            else:
                self.sample_distance_progress = [0.0 for _ in sample_distances_m]
        else:
            self.sample_distance_progress = []

        self.refresh_sample_caches()

    def build_sample_elapsed_seconds(self, timestamps):
        if not timestamps:
            return []

        origin = timestamps[0]
        elapsed_seconds = [0.0]
        last_value = 0.0
        for index, timestamp in enumerate(timestamps[1:], start=1):
            if origin is None or timestamp is None:
                current_value = float(index)
            else:
                current_value = max(0.0, (timestamp - origin).total_seconds())
            if current_value <= last_value:
                current_value = last_value + 1e-3
            elapsed_seconds.append(current_value)
            last_value = current_value
        return elapsed_seconds

    def refresh_sample_caches(self):
        self.sample_course_points = list(getattr(self, constant.ATTR_COURSE, []))
        self.sample_elevations = list(getattr(self, constant.ATTR_ELEVATION, []))

    def duration_seconds(self):
        return max(0.0, self.trim_end_seconds - self.trim_start_seconds)

    def integer_duration_seconds(self):
        return (
            max(1, int(self.duration_seconds())) if self.duration_seconds() > 0 else 0
        )

    def interpolate_numeric_value(self, x_values, y_values, target_x):
        if not x_values or not y_values:
            return 0.0
        if len(y_values) == 1 or target_x <= x_values[0]:
            return y_values[0]
        if target_x >= x_values[-1]:
            return y_values[-1]

        right_index = bisect_left(x_values, target_x)
        if x_values[right_index] == target_x:
            return y_values[right_index]

        left_index = right_index - 1
        left_x = x_values[left_index]
        right_x = x_values[right_index]
        if right_x <= left_x:
            return y_values[right_index]

        ratio = (target_x - left_x) / (right_x - left_x)
        left_y = y_values[left_index]
        right_y = y_values[right_index]
        return left_y + (right_y - left_y) * ratio

    def interpolate_attribute_value(self, attribute, x_values, y_values, target_x):
        if attribute == constant.ATTR_TIME:
            if self.source_start_time is None:
                return y_values[0] if y_values else None
            return self.source_start_time + timedelta(seconds=target_x)

        if attribute == constant.ATTR_COURSE:
            latitudes = [point[0] for point in y_values]
            longitudes = [point[1] for point in y_values]
            return (
                self.interpolate_numeric_value(x_values, latitudes, target_x),
                self.interpolate_numeric_value(x_values, longitudes, target_x),
            )

        return self.interpolate_numeric_value(x_values, y_values, target_x)

    def interpolate(self, fps: int):
        def helper(x_values, data, target_x_values):
            from scipy.interpolate import interp1d

            if not data:
                return []
            if len(data) == 1 or len(target_x_values) == 0:
                return [data[0] for _ in target_x_values]

            interp_func = interp1d(
                x_values,
                data,
                bounds_error=False,
                fill_value=(data[0], data[-1]),
                assume_sorted=True,
            )
            return interp_func(target_x_values).tolist()

        if fps <= 0:
            raise ValueError(f"Invalid fps: {fps}")

        if self.sample_elapsed_seconds and len(self.sample_elapsed_seconds) >= 2:
            import numpy as np

            target_x_values = np.arange(
                self.trim_start_seconds,
                self.trim_end_seconds,
                1 / fps,
            )
            self.frame_elapsed_seconds = [
                target_x - self.trim_start_seconds
                for target_x in target_x_values.tolist()
            ]
            if self.source_start_time is not None:
                self.frame_timestamps = [
                    self.source_start_time + timedelta(seconds=target_x)
                    for target_x in target_x_values.tolist()
                ]
            else:
                self.frame_timestamps = []

            if self.sample_distance_progress:
                self.frame_distance_progress = helper(
                    self.sample_elapsed_seconds,
                    self.sample_distance_progress,
                    target_x_values,
                )
            else:
                self.frame_distance_progress = []

            for attribute in self.valid_attributes:
                if attribute in constant.NO_INTERPOLATE_ATTRIBUTES:
                    continue
                data = getattr(self, attribute)
                if attribute == constant.ATTR_COURSE:
                    new_lat = helper(
                        self.sample_elapsed_seconds,
                        [point[0] for point in data],
                        target_x_values,
                    )
                    new_lon = helper(
                        self.sample_elapsed_seconds,
                        [point[1] for point in data],
                        target_x_values,
                    )
                    new_data = list(zip(new_lat, new_lon))
                else:
                    new_data = helper(
                        self.sample_elapsed_seconds,
                        data,
                        target_x_values,
                    )
                setattr(self, attribute, new_data)
            return

        for attribute in self.valid_attributes:
            if attribute in constant.NO_INTERPOLATE_ATTRIBUTES:
                continue
            data = getattr(self, attribute)
            x_values = list(range(len(data)))
            target_x_values = [
                frame_index / fps for frame_index in range(max(0, len(data) * fps))
            ]
            if attribute == constant.ATTR_COURSE:
                new_lat = helper(x_values, [ele[0] for ele in data], target_x_values)
                new_lon = helper(x_values, [ele[1] for ele in data], target_x_values)
                new_data = list(zip(new_lat, new_lon))
            else:
                new_data = helper(x_values, data, target_x_values)
            setattr(self, attribute, new_data)

        fallback_frame_count = 0
        for attribute in self.valid_attributes:
            if attribute in constant.NO_INTERPOLATE_ATTRIBUTES:
                continue
            fallback_frame_count = len(getattr(self, attribute))
            break
        self.frame_elapsed_seconds = [
            frame_index / fps for frame_index in range(fallback_frame_count)
        ]
        self.frame_timestamps = []
        self.frame_distance_progress = []

    def trim(self, start, end):
        if self.sample_elapsed_seconds:
            duration_seconds = self.duration_seconds()
            if start < 0 or start >= duration_seconds:
                raise ValueError(
                    f"Invalid scene start value in config. Value should be at least 0 and less than {duration_seconds:.3f}. Current value is {start}"
                )
            if end <= start or end > duration_seconds:
                raise ValueError(
                    f"Invalid scene end value in config. Value should be at most {duration_seconds:.3f} and greater than {start}. Current value is {end}"
                )

            source_elapsed_seconds = list(self.sample_elapsed_seconds)
            source_start_time = self.source_start_time
            start_inner_index = bisect_right(source_elapsed_seconds, start)
            end_inner_index = bisect_left(source_elapsed_seconds, end)

            trimmed_elapsed_seconds = [start]
            trimmed_elapsed_seconds.extend(
                source_elapsed_seconds[start_inner_index:end_inner_index]
            )
            trimmed_elapsed_seconds.append(end)

            for attribute in self.valid_attributes:
                source_data = list(getattr(self, attribute))
                start_value = self.interpolate_attribute_value(
                    attribute,
                    source_elapsed_seconds,
                    source_data,
                    start,
                )
                end_value = self.interpolate_attribute_value(
                    attribute,
                    source_elapsed_seconds,
                    source_data,
                    end,
                )
                trimmed_data = [start_value]
                trimmed_data.extend(source_data[start_inner_index:end_inner_index])
                trimmed_data.append(end_value)
                setattr(self, attribute, trimmed_data)

            if self.sample_distance_progress:
                source_distance_progress = list(self.sample_distance_progress)
                start_progress = self.interpolate_numeric_value(
                    source_elapsed_seconds,
                    source_distance_progress,
                    start,
                )
                end_progress = self.interpolate_numeric_value(
                    source_elapsed_seconds,
                    source_distance_progress,
                    end,
                )
                trimmed_distance_progress = [start_progress]
                trimmed_distance_progress.extend(
                    source_distance_progress[start_inner_index:end_inner_index]
                )
                trimmed_distance_progress.append(end_progress)
                progress_span = max(end_progress - start_progress, 1e-9)
                self.sample_distance_progress = [
                    (progress_value - start_progress) / progress_span
                    for progress_value in trimmed_distance_progress
                ]
            else:
                self.sample_distance_progress = []

            self.sample_elapsed_seconds = trimmed_elapsed_seconds
            if source_start_time is not None:
                self.source_start_time = source_start_time + timedelta(seconds=start)
            self.trim_end_seconds = end - start
            self.trim_start_seconds = 0.0
            self.sample_elapsed_seconds = [
                elapsed_second - start for elapsed_second in self.sample_elapsed_seconds
            ]
            self.refresh_sample_caches()
            return

        for attribute in self.valid_attributes:
            data = getattr(self, attribute)
            if start > len(data):
                raise ValueError(
                    f"Invalid scene start value in config. Value should be less than {len(data)}. Current value is {start}"
                )
            if end > len(data) or end < start:
                raise ValueError(
                    f"Invalid scene end value in config. Value should be at most {len(data)} and greater than {start}. Current value is {end}"
                )
            setattr(self, attribute, data[start:end])

        self.trim_start_seconds = 0.0
        self.trim_end_seconds = max(0.0, float(end - start))
        self.sample_elapsed_seconds = [float(index) for index in range(end - start)]
        self.sample_distance_progress = []
        self.refresh_sample_caches()

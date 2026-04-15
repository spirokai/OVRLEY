import math
import os
from datetime import timedelta
from time import perf_counter

import constant

# Lazy imports for:
# from PIL import Image, ImageColor, ImageDraw, ImageFont
# from plot import build_image


class Frame:
    def __init__(self, filename, width, height, second, frame_number):
        self.filename = filename
        self.width = width
        self.height = height
        self.second = second
        self.frame_number = frame_number

    def full_path(self):
        return f"{constant.FRAMES_DIR()}{self.filename}"

    def get_cached_font(self, font_cache, font_name, font_size):
        from PIL import ImageFont

        resolved_font = font_name
        if not os.path.exists(resolved_font):
            resolved_font = constant.FONTS_DIR() + font_name

        if font_cache is None:
            return ImageFont.truetype(resolved_font, font_size)

        cache_key = (resolved_font, int(font_size))
        if cache_key not in font_cache.by_key:
            font_cache.by_key[cache_key] = ImageFont.truetype(resolved_font, font_size)
        return font_cache.by_key[cache_key]

    def draw_value(
        self,
        img,
        value: str,
        config: dict,
        scene_config: dict = None,
        font_cache=None,
    ):
        from PIL import ImageColor, ImageDraw

        def hex_color_with_alpha(color, opacity):
            if opacity is None:
                return color
            int_value = round(opacity * 255)
            hex_string = f"{int_value:02x}"
            return color + hex_string

        # Get decimal_rounding from config or scene_config
        decimal_rounding = config.get("decimal_rounding")
        if decimal_rounding is None and scene_config:
            decimal_rounding = scene_config.get("decimal_rounding")

        if type(value) in (int, float):
            if decimal_rounding is not None:
                if decimal_rounding == 0:
                    value = int(value)
                else:
                    value = round(
                        float(value), decimal_rounding
                    )  # TODO - should pad right side with 0s so less jumpy suffix
        value = str(value)
        if "suffix" in config.keys():
            value += config["suffix"]

        # Get font from config or scene_config
        font = config.get("font")
        if font is None and scene_config:
            font = scene_config.get("font", "Arial.ttf")
        else:
            font = font or "Arial.ttf"

        # Get font size from config or scene_config
        font_size = config.get("font_size")
        if font_size is None and scene_config:
            font_size = scene_config.get("font_size", 32)
        else:
            font_size = font_size or 32

        font_obj = self.get_cached_font(font_cache, font, font_size)
        ImageDraw.Draw(img).text(
            (config["x"], config["y"]),
            value,
            font=font_obj,
            fill=ImageColor.getcolor(
                hex_color_with_alpha(
                    config.get("color", constant.DEFAULT_COLOR),
                    config["opacity"] if "opacity" in config.keys() else None,
                ),
                "RGBA",
            ),
        )
        return img

    def draw_figure(
        self,
        img,
        config,
        attribute,
        figure,
        fps=None,
        render_profiler=None,
        profile_stage=None,
    ):
        raise ValueError(
            "Legacy draw_figure path is no longer supported. "
            "Route and elevation must use cached compositing."
        )

    def draw_cached_text(self, img, text, x, y, label_style, font_cache=None):
        from PIL import ImageColor, ImageDraw

        if not text or label_style is None:
            return img

        font_obj = self.get_cached_font(
            font_cache,
            label_style.font_path,
            label_style.font_size,
        )
        ImageDraw.Draw(img).text(
            (x, y),
            text,
            font=font_obj,
            fill=ImageColor.getcolor(label_style.color, "RGBA"),
        )
        return img

    def paste_widget_marker(
        self,
        img,
        sprite,
        marker_anchor,
        widget_x,
        widget_y,
        widget_width,
        widget_height,
        marker_x,
        marker_y,
        rotation_deg,
    ):
        from PIL import Image

        if sprite is None:
            return img

        if rotation_deg == 0:
            marker_left = round(widget_x + marker_x - marker_anchor[0])
            marker_top = round(widget_y + marker_y - marker_anchor[1])
            img.paste(sprite, (marker_left, marker_top), sprite)
            return img

        marker_layer = Image.new("RGBA", (widget_width, widget_height), (0, 0, 0, 0))
        marker_left = round(marker_x - marker_anchor[0])
        marker_top = round(marker_y - marker_anchor[1])
        marker_layer.paste(sprite, (marker_left, marker_top), sprite)
        rotated_marker_layer = marker_layer.rotate(rotation_deg, resample=3, expand=True)
        img.paste(rotated_marker_layer, (widget_x, widget_y), rotated_marker_layer)
        return img

    def transform_rotated_point(self, x_value, y_value, width, height, rotation_deg):
        angle_rad = math.radians(rotation_deg)
        center_x = width / 2.0
        center_y = height / 2.0
        translated_x = x_value - center_x
        translated_y = y_value - center_y

        rotated_x = translated_x * math.cos(angle_rad) - translated_y * math.sin(angle_rad)
        rotated_y = translated_x * math.sin(angle_rad) + translated_y * math.cos(angle_rad)

        corners = [
            (-center_x, -center_y),
            (width - center_x, -center_y),
            (-center_x, height - center_y),
            (width - center_x, height - center_y),
        ]
        rotated_corners = [
            (
                corner_x * math.cos(angle_rad) - corner_y * math.sin(angle_rad),
                corner_x * math.sin(angle_rad) + corner_y * math.cos(angle_rad),
            )
            for corner_x, corner_y in corners
        ]
        min_x = min(corner_x for corner_x, _ in rotated_corners)
        min_y = min(corner_y for _, corner_y in rotated_corners)

        return rotated_x - min_x, rotated_y - min_y

    def composite_elevation_widget(self, img, elevation_cache, render_assets=None, render_profiler=None):
        if elevation_cache is None or not elevation_cache.frame_states:
            return img

        state_index = min(
            self.second * getattr(self, "fps", 0) + self.frame_number,
            len(elevation_cache.frame_states) - 1,
        )
        if state_index < 0:
            state_index = 0
        state = elevation_cache.frame_states[state_index]

        label_style = elevation_cache.label_style
        font_cache = render_assets.font_cache if render_assets is not None else None

        if elevation_cache.rotation_deg == 0:
            background_layer = elevation_cache.background_layer
            completed_layer = elevation_cache.completed_layer
            widget_x = elevation_cache.widget_x
            widget_y = elevation_cache.widget_y
            marker_x = state.marker_x
            marker_y = state.marker_y
        else:
            background_layer = (
                elevation_cache.rotated_background_layer or elevation_cache.background_layer
            )
            completed_layer = (
                elevation_cache.rotated_completed_layer or elevation_cache.completed_layer
            )
            widget_x = elevation_cache.widget_x
            widget_y = elevation_cache.widget_y
            marker_x, marker_y = self.transform_rotated_point(
                state.marker_x,
                state.marker_y,
                elevation_cache.widget_width,
                elevation_cache.widget_height,
                elevation_cache.rotation_deg,
            )

        if background_layer is not None:
            img.paste(background_layer, (widget_x, widget_y), background_layer)

        if completed_layer is not None:
            reveal_width = max(
                1,
                min(completed_layer.width, round(completed_layer.width * state.progress01)),
            )
            completed_crop = completed_layer.crop((0, 0, reveal_width, completed_layer.height))
            img.paste(completed_crop, (widget_x, widget_y), completed_crop)

        if elevation_cache.marker_sprite is not None:
            img = self.paste_widget_marker(
                img,
                elevation_cache.marker_sprite,
                elevation_cache.marker_anchor,
                widget_x,
                widget_y,
                elevation_cache.widget_width,
                elevation_cache.widget_height,
                state.marker_x,
                state.marker_y,
                elevation_cache.rotation_deg,
            )

        if state.label_text and label_style is not None:
            label_x = round(widget_x + marker_x + label_style.x_offset)
            label_y = round(widget_y + marker_y + label_style.y_offset)
            start = perf_counter()
            img = self.draw_cached_text(
                img,
                state.label_text,
                label_x,
                label_y,
                label_style,
                font_cache=font_cache,
            )
            if render_profiler:
                render_profiler.record("text.elevation_label", perf_counter() - start)

        return img

    def append_route_point(self, points, point):
        if not points:
            points.append(point)
            return

        last_x, last_y = points[-1]
        point_x, point_y = point
        if abs(last_x - point_x) > 1e-6 or abs(last_y - point_y) > 1e-6:
            points.append(point)

    def build_route_prefix_points(self, route_cache, state):
        points = list(route_cache.display_points[: state.segment_index])
        self.append_route_point(points, (state.marker_x, state.marker_y))
        return points

    def build_route_delta_points(self, route_cache, previous_state, state):
        points = []
        self.append_route_point(points, (previous_state.marker_x, previous_state.marker_y))

        for point in route_cache.display_points[previous_state.segment_index : state.segment_index]:
            self.append_route_point(points, point)

        self.append_route_point(points, (state.marker_x, state.marker_y))
        return points

    def update_route_reveal_mask(self, route_cache, state_index, completed_layer):
        from PIL import Image, ImageDraw

        if completed_layer is None:
            return None

        if (
            route_cache.reveal_mask is None
            or route_cache.reveal_mask.size != completed_layer.size
        ):
            route_cache.reveal_mask = Image.new("L", completed_layer.size, 0)
            route_cache.last_revealed_state_index = -1

        state = route_cache.frame_states[state_index]
        draw = ImageDraw.Draw(route_cache.reveal_mask)

        if state_index != route_cache.last_revealed_state_index + 1:
            route_cache.reveal_mask = Image.new("L", completed_layer.size, 0)
            draw = ImageDraw.Draw(route_cache.reveal_mask)
            points = self.build_route_prefix_points(route_cache, state)
        elif route_cache.last_revealed_state_index >= 0:
            previous_state = route_cache.frame_states[route_cache.last_revealed_state_index]
            points = self.build_route_delta_points(route_cache, previous_state, state)
        else:
            points = self.build_route_prefix_points(route_cache, state)

        if len(points) >= 2:
            draw.line(
                points,
                fill=255,
                width=max(1, round(route_cache.line_width)),
                joint="curve",
            )

        route_cache.last_revealed_state_index = state_index
        return route_cache.reveal_mask

    def composite_route_widget(self, img, route_cache, render_assets=None, render_profiler=None):
        if route_cache is None or not route_cache.frame_states:
            return img

        state_index = min(
            self.second * getattr(self, "fps", 0) + self.frame_number,
            len(route_cache.frame_states) - 1,
        )
        if state_index < 0:
            state_index = 0
        state = route_cache.frame_states[state_index]

        if route_cache.rotation_deg == 0:
            background_layer = route_cache.background_layer
            completed_layer = route_cache.completed_layer
            widget_x = route_cache.widget_x
            widget_y = route_cache.widget_y
            marker_x = state.marker_x
            marker_y = state.marker_y
        else:
            background_layer = route_cache.rotated_background_layer or route_cache.background_layer
            completed_layer = route_cache.rotated_completed_layer or route_cache.completed_layer
            widget_x = route_cache.widget_x
            widget_y = route_cache.widget_y
            marker_x = state.marker_x
            marker_y = state.marker_y

        if background_layer is not None:
            img.paste(background_layer, (widget_x, widget_y), background_layer)

        reveal_mask = self.update_route_reveal_mask(route_cache, state_index, completed_layer)
        if completed_layer is not None and reveal_mask is not None:
            img.paste(completed_layer, (widget_x, widget_y), reveal_mask)

        if route_cache.marker_sprite is not None:
            img = self.paste_widget_marker(
                img,
                route_cache.marker_sprite,
                route_cache.marker_anchor,
                widget_x,
                widget_y,
                route_cache.widget_width,
                route_cache.widget_height,
                marker_x,
                marker_y,
                0,
            )

        return img

    def draw(
        self,
        configs,
        figures,
        render_assets=None,
        base_image=None,
        plot_backgrounds=None,
        render_profiler=None,
    ):
        """
        Draw the frame. If base_image is provided (with static labels),
        it will be used as a starting point. If plot_backgrounds is provided,
        those will be composited with dynamic position markers.
        """

        def convert_value(value, attribute, config):
            unit = config["unit"]
            match attribute:
                case constant.ATTR_SPEED:
                    if unit == constant.UNIT_IMPERIAL:
                        value *= constant.MPH_CONVERSION
                    elif unit == constant.UNIT_METRIC:
                        value *= constant.KMH_CONVERSION
                    else:
                        raise ValueError(f"Unknown unit: {unit}")
                case constant.ATTR_ELEVATION:
                    if unit == "imperial":
                        value *= constant.FT_CONVERSION
                    elif unit == "metric":
                        pass
                    else:
                        raise ValueError(f"Unknown unit: {unit}")
                case constant.ATTR_TIME:
                    # TODO - try to use timezone instead of offset. maybe? idk if this is a good TODO
                    hours_offset = config["hours_offset"]
                    time_format = config["time_format"]
                    value += timedelta(hours=hours_offset)
                    value = value.strftime(time_format)
            return value

        if render_assets is not None:
            base_image = render_assets.base_image
            font_cache = render_assets.font_cache
            self.fps = configs["scene"]["fps"]
        else:
            font_cache = None

        # Use base_image if provided, otherwise create new
        if base_image is not None:
            img = base_image.copy()
        else:
            from PIL import Image

            img = Image.new("RGBA", (self.width, self.height))

        scene_config = configs.get("scene", {})

        # Only draw dynamic values, skip labels and plots if base_image provided
        if "values" in configs.keys():
            for config in configs["values"]:
                attribute = config["value"]
                if attribute in self.valid_attributes:
                    value = getattr(self, attribute)
                    if (
                        "unit" in config.keys()
                        or ("hours_offset" and "time_format") in config.keys()
                    ):
                        value = convert_value(value, attribute, config)
                    start = perf_counter()
                    img = self.draw_value(
                        img,
                        value,
                        config,
                        scene_config,
                        font_cache=font_cache,
                    )
                    if render_profiler:
                        render_profiler.record(
                            "text.dynamic", perf_counter() - start
                        )

        # Only draw static elements if no base_image provided
        if base_image is None:
            if "labels" in configs.keys():
                for config in configs["labels"]:
                    start = perf_counter()
                    img = self.draw_value(
                        img,
                        config["text"],
                        config,
                        scene_config,
                        font_cache=font_cache,
                    )
                    if render_profiler:
                        render_profiler.record(
                            "text.static", perf_counter() - start
                        )
            if "plots" in configs.keys():
                for config in configs["plots"]:
                    attribute = config["value"]
                    if attribute == constant.ATTR_COURSE and render_assets is not None:
                        start = perf_counter()
                        img = self.composite_route_widget(
                            img,
                            render_assets.route_cache,
                            render_assets=render_assets,
                            render_profiler=render_profiler,
                        )
                        if render_profiler:
                            render_profiler.record("composite.route", perf_counter() - start)
                        continue
                    if attribute == constant.ATTR_ELEVATION and render_assets is not None:
                        start = perf_counter()
                        img = self.composite_elevation_widget(
                            img,
                            render_assets.elevation_cache,
                            render_assets=render_assets,
                            render_profiler=render_profiler,
                        )
                        if render_profiler:
                            render_profiler.record(
                                "composite.elevation", perf_counter() - start
                            )
                        continue
                    raise ValueError(
                        f"Legacy plot rendering is no longer supported for '{attribute}'."
                    )
        else:
            if render_assets is not None and render_assets.route_cache is not None:
                start = perf_counter()
                img = self.composite_route_widget(
                    img,
                    render_assets.route_cache,
                    render_assets=render_assets,
                    render_profiler=render_profiler,
                )
                if render_profiler:
                    render_profiler.record("composite.route", perf_counter() - start)
            if render_assets is not None and render_assets.elevation_cache is not None:
                start = perf_counter()
                img = self.composite_elevation_widget(
                    img,
                    render_assets.elevation_cache,
                    render_assets=render_assets,
                    render_profiler=render_profiler,
                )
                if render_profiler:
                    render_profiler.record("composite.elevation", perf_counter() - start)
        return img

    def profile_label_text(self, config):
        text = ""
        for unit in config["units"]:
            value = self.elevation * constant.ELEVATION_CONVERSION_MAP[unit]
            if "decimal_rounding" in config.keys():
                if config["decimal_rounding"] == 0:
                    value = int(value)
                else:
                    value = round(float(value), config["decimal_rounding"])
            text += (
                f"{value}{constant.DEFAULT_SUFFIX_MAP[constant.ATTR_ELEVATION][unit]}\n"
            )
        return text.rstrip()

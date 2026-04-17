import math
import os
import shutil
from subprocess import PIPE, Popen
import logging
from datetime import datetime, timedelta
from time import perf_counter

from plot import (
    get_line_color,
    get_line_width,
    get_opacity,
    get_point_color,
    get_point_weight,
)

import constant


def resolve_ffmpeg_binary():
    import sys

    candidate_paths = []

    env_override = os.environ.get("CYCLEMETRY_FFMPEG") or os.environ.get(
        "FFMPEG_BINARY"
    )
    if env_override:
        candidate_paths.append(env_override)

    if getattr(sys, "frozen", False):
        bundled_name = "ffmpeg.exe" if os.name == "nt" else "ffmpeg"
        candidate_paths.append(os.path.join(sys._MEIPASS, bundled_name))

    local_name = "ffmpeg.exe" if os.name == "nt" else "ffmpeg"
    candidate_paths.append(os.path.join(os.path.dirname(__file__), local_name))

    for candidate in candidate_paths:
        if candidate and os.path.isfile(candidate):
            return candidate

    system_ffmpeg = shutil.which("ffmpeg")
    if system_ffmpeg:
        return system_ffmpeg

    try:
        from imageio_ffmpeg import get_ffmpeg_exe

        bundled_ffmpeg = get_ffmpeg_exe()
        if bundled_ffmpeg and os.path.isfile(bundled_ffmpeg):
            return bundled_ffmpeg
    except Exception:
        pass

    raise FileNotFoundError(
        "ffmpeg executable not found. Install ffmpeg and add it to PATH, "
        "set CYCLEMETRY_FFMPEG to its full path, or install the Python "
        "package imageio-ffmpeg."
    )


# Lazy imports for:
# from frame import Frame
# from plot import build_figure
# from template import build_configs


class Scene:
    def __init__(self, activity, template):
        self.activity = activity
        self.template = template
        self.fps = self.template["scene"]["fps"]
        if "labels" in self.template.keys():
            self.labels = self.template["labels"]
        else:
            self.labels = []
        self.frames = []

    def render_video(self, seconds, progress_callback=None, cancel_check=None):
        self.export_video(seconds, progress_callback, cancel_check)

    def render_demo(self, seconds, second):
        import matplotlib.pyplot as plt

        try:
            total_frames = self.total_frame_count(seconds)
            frame_index = min(
                max(0, round(second * self.fps)), max(total_frames - 1, 0)
            )
            frame_second, frame_number = self.frame_time_components(frame_index)
            self.frames.append(
                self.build_frame(total_frames, frame_second, frame_number)
            )
            self.draw_frames()
        finally:
            # Always close all matplotlib figures to prevent memory leaks
            plt.close("all")
            # Clear any figure references
            if hasattr(self, "figs") and self.figs:
                for fig in self.figs:
                    plt.close(fig)
                self.figs = None

    def update_configs(self, config_filename):
        from template import build_configs

        self.template = build_configs(config_filename)

    def draw_frames(self):
        from render_debug import RenderDebugOptions

        if not os.path.exists(constant.FRAMES_DIR()):
            os.makedirs(constant.FRAMES_DIR())
        if not hasattr(self, "figs"):
            self.figs = None
        render_debug = RenderDebugOptions.from_scene_config(self.template.get("scene"))
        render_debug.ensure_output_dir()
        render_assets = self.prepare_render_assets(render_debug=render_debug)
        for frame in self.frames:
            frame.draw(
                self.template,
                self.figs,
                render_assets=render_assets,
            ).save(frame.full_path())

    def build_figures(self):
        self.figs = {}
        for config in self.template.get("plots", []):
            attribute = config["value"]
            if attribute in {constant.ATTR_COURSE, constant.ATTR_ELEVATION}:
                continue
            raise ValueError(
                f"Legacy plot figure generation is no longer supported for '{attribute}'."
            )

    def append_ffmpeg_option(self, args, flag, value):
        if value is None or value == "":
            return
        args.extend([flag, str(value)])

    def build_ffmpeg_settings(self, overlay_filename):
        scene_config = self.template.get("scene", {})
        ffmpeg_config = scene_config.get("ffmpeg", {})
        if not isinstance(ffmpeg_config, dict):
            ffmpeg_config = {}

        codec_name = ffmpeg_config.get("codec", "prores_ks")
        loglevel = str(ffmpeg_config.get("loglevel", "info"))
        pixel_format_out = ffmpeg_config.get("pix_fmt", "yuva444p10le")

        output_args = ["-c:v", str(codec_name)]
        self.append_ffmpeg_option(output_args, "-threads", ffmpeg_config.get("threads"))

        if codec_name == "prores_ks":
            self.append_ffmpeg_option(
                output_args, "-profile:v", ffmpeg_config.get("prores_profile")
            )
            self.append_ffmpeg_option(
                output_args, "-qscale:v", ffmpeg_config.get("qscale")
            )
            self.append_ffmpeg_option(
                output_args, "-bits_per_mb", ffmpeg_config.get("bits_per_mb")
            )
            self.append_ffmpeg_option(
                output_args, "-mbs_per_slice", ffmpeg_config.get("mbs_per_slice")
            )
            self.append_ffmpeg_option(
                output_args, "-vendor", ffmpeg_config.get("vendor")
            )
            self.append_ffmpeg_option(
                output_args, "-alpha_bits", ffmpeg_config.get("alpha_bits")
            )

        extra_output_args = ffmpeg_config.get("output_args", [])
        if isinstance(extra_output_args, (list, tuple)):
            output_args.extend(str(arg) for arg in extra_output_args)

        output_args.extend(["-pix_fmt", str(pixel_format_out), "-y", overlay_filename])
        return {
            "loglevel": loglevel,
            "output_args": output_args,
        }

    def export_video(self, seconds, progress_callback=None, cancel_check=None):
        from render_debug import (
            RenderDebugOptions,
            RenderProfiler,
            build_timing_payload,
        )

        export_started_at = perf_counter()

        overlay_filename = (
            self.template["scene"]["overlay_filename"]
            if "overlay_filename" in self.template["scene"].keys()
            else constant.DEFAULT_OVERLAY_FILENAME
        )
        if not os.path.isabs(overlay_filename):
            overlay_filename = os.path.join(constant.WRITE_DIR(), overlay_filename)
        width, height = (
            self.template["scene"]["width"],
            self.template["scene"]["height"],
        )
        frame_queue_maxsize = 4

        # Ensure dimensions are even (required by most codecs including ProRes)
        if width % 2 != 0:
            width += 1
        if height % 2 != 0:
            height += 1

        render_debug = RenderDebugOptions.from_scene_config(self.template.get("scene"))
        render_profiler = RenderProfiler()
        render_debug.ensure_output_dir()
        render_assets = self.prepare_render_assets(
            width=width,
            height=height,
            render_debug=render_debug,
            render_profiler=render_profiler,
        )
        render_assets.initialize_frame_buffer_pool(frame_queue_maxsize + 1)
        total_frames = self.total_frame_count(seconds)

        # FFmpeg command to encode video from raw frames
        # Input parameters (must come before -i)
        framerate = ["-r", str(self.fps)]
        fmt = ["-f", "rawvideo"]
        pixel_format_in = ["-pix_fmt", "rgba"]
        size = ["-s", f"{width}x{height}"]

        ffmpeg_settings = self.build_ffmpeg_settings(overlay_filename)

        ffmpeg_bin = resolve_ffmpeg_binary()
        logging.info(f"Using ffmpeg binary: {ffmpeg_bin}")

        ffmpeg_cmd = (
            [ffmpeg_bin]
            + ["-loglevel", ffmpeg_settings["loglevel"]]
            + fmt
            + size
            + pixel_format_in
            + framerate
            + ["-i", "-"]
            + ffmpeg_settings["output_args"]
        )

        logging.info(
            f"Starting ffmpeg with dimensions {width}x{height} and command: {' '.join(ffmpeg_cmd)}"
        )

        env = os.environ.copy()
        if os.name != "nt":
            extra_paths = [
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
            ]
            current_path = env.get("PATH", "")
            env["PATH"] = os.pathsep.join(
                extra_paths + ([current_path] if current_path else [])
            )

        try:
            p = Popen(ffmpeg_cmd, stdin=PIPE, stderr=PIPE, stdout=PIPE, env=env)
        except Exception as e:
            logging.error(f"Failed to start ffmpeg process: {e}")
            raise Exception(f"Could not start ffmpeg: {str(e)}")
        # Threaded monitoring of ffmpeg stderr to track progress and prevent deadlocks
        import queue
        import threading
        import re

        encoded_frames = 0
        io_lock = threading.Lock()
        stop_event = threading.Event()
        frame_queue = queue.Queue(maxsize=frame_queue_maxsize)
        queue_sentinel = object()
        encoder_error = {"exception": None}

        def set_encoder_error(exception):
            with io_lock:
                if encoder_error["exception"] is None:
                    encoder_error["exception"] = exception
            stop_event.set()

        def raise_if_encoder_failed():
            with io_lock:
                exception = encoder_error["exception"]
            if exception is not None:
                raise exception

        def queue_frame_payload(payload):
            while True:
                raise_if_encoder_failed()
                if stop_event.is_set():
                    raise Exception(
                        "Rendering stopped before frame payload could be queued"
                    )
                try:
                    with render_profiler.measure("queue.put_wait"):
                        frame_queue.put(payload, timeout=0.1)
                    return
                except queue.Full:
                    continue

        def signal_encoder_shutdown():
            while encoder_thread.is_alive():
                try:
                    frame_queue.put(queue_sentinel, timeout=0.1)
                    return
                except queue.Full:
                    raise_if_encoder_failed()
                    continue

        def monitor_ffmpeg(process):
            nonlocal encoded_frames

            # Regex to find "frame= 123"
            frame_pattern = re.compile(r"frame=\s*(\d+)")

            while True:
                line = process.stderr.readline()
                if not line:
                    break

                line_str = line.decode("utf-8", errors="replace")

                # Update progress
                match = frame_pattern.search(line_str)
                if match:
                    with io_lock:
                        encoded_frames = int(match.group(1))

                # Log only errors or warnings, avoid spamming info
                if "error" in line_str.lower() or "warning" in line_str.lower():
                    logging.info(f"ffmpeg: {line_str.strip()}")

        def encoder_worker(process):
            try:
                while True:
                    with render_profiler.measure("encoder.queue_wait"):
                        payload = frame_queue.get()
                    try:
                        if payload is queue_sentinel:
                            if process.stdin and not process.stdin.closed:
                                process.stdin.close()
                            return
                        with render_profiler.measure("encoder.serialize"):
                            frame_bytes = payload.tobytes()
                        with render_profiler.measure("ffmpeg.write"):
                            process.stdin.write(frame_bytes)
                    finally:
                        if payload is not queue_sentinel:
                            if not render_assets.release_frame_image(
                                payload
                            ) and hasattr(payload, "close"):
                                payload.close()
                        frame_queue.task_done()
            except BrokenPipeError:
                logging.error("Broken pipe when writing to ffmpeg")
                set_encoder_error(
                    Exception("ffmpeg pipe broken - video encoding failed")
                )
            except Exception as exception:
                logging.error(f"Encoder thread failed: {exception}")
                set_encoder_error(exception)

        monitor_thread = threading.Thread(target=monitor_ffmpeg, args=(p,), daemon=True)
        monitor_thread.start()
        encoder_thread = threading.Thread(target=encoder_worker, args=(p,), daemon=True)
        encoder_thread.start()

        # Render on the main thread and feed a bounded queue consumed by the encoder thread.
        logging.info(f"Rendering {total_frames} frames with bounded encode queue")
        sample_frame_indices = render_debug.sample_frame_indices(total_frames)
        rendered_frames = 0

        try:
            for idx, frame in enumerate(self.iter_frames(seconds)):
                frame_loop_start = perf_counter()

                # Check for cancellation
                if cancel_check and cancel_check():
                    logging.info("Rendering cancelled by user")
                    raise Exception("Rendering cancelled by user")

                # Check if ffmpeg is still alive
                if p.poll() is not None:
                    logging.error(f"ffmpeg process died unexpectedly at frame {idx}")
                    raise Exception(f"ffmpeg died (exit {p.returncode})")

                raise_if_encoder_failed()

                try:
                    with render_profiler.measure("frame.draw"):
                        image = frame.draw(
                            self.template,
                            self.figs,
                            render_assets=render_assets,
                            render_profiler=render_profiler,
                        )
                    if idx in sample_frame_indices:
                        render_debug.save_sample_frame(idx, image)
                    queue_frame_payload(image)
                    rendered_frames += 1
                finally:
                    render_profiler.record(
                        "frame.total", perf_counter() - frame_loop_start
                    )

                # Progress callback
                if progress_callback:
                    current_enc = 0
                    with io_lock:
                        current_enc = encoded_frames

                    # Pass both generation progress and encoding progress
                    # Callback signature: (current_gen, total_gen, current_enc)
                    try:
                        progress_callback(idx + 1, total_frames, current_enc)
                    except TypeError:
                        # Fallback for old signature
                        progress_callback(idx + 1, total_frames)

            signal_encoder_shutdown()
            encoder_thread.join()
            raise_if_encoder_failed()
        except Exception:
            stop_event.set()
            try:
                signal_encoder_shutdown()
            except Exception:
                pass
            if p.poll() is None:
                p.terminate()
                p.wait()
            monitor_thread.join(timeout=1)
            encoder_thread.join(timeout=1)
            if os.path.exists(overlay_filename):
                os.remove(overlay_filename)
            raise

        # Wait for ffmpeg to finish
        p.wait()
        monitor_thread.join()
        return_code = p.returncode

        # Check if ffmpeg succeeded
        if return_code != 0:
            logging.error(f"ffmpeg failed with exit code {return_code}")
            raise Exception(f"ffmpeg encoding failed (exit {return_code})")

        logging.info(f"ffmpeg completed successfully, output: {overlay_filename}")
        timing_payload = build_timing_payload(
            render_profiler,
            self.template.get("scene", {}),
            overlay_filename,
            total_frames,
            rendered_frames,
            sample_frame_indices,
            total_time_taken=perf_counter() - export_started_at,
        )
        logging.info("Render timing summary: %s", timing_payload["timings"])
        if render_debug.write_timing_summary:
            render_debug.save_json("timing_summary.json", timing_payload)

        # TODO - try to not depend on ffmpeg subprocess call please
        # clips = [
        #     ImageClip(frame.filename, transparent=True).set_duration(frame_duration)
        #     for frame in self.frames
        # ]
        # concatenate_videoclips(clips, method="compose").write_videofile(
        #     export_filename,
        #     codec="mpeg4",
        #     ffmpeg_params=["-pix_fmt", "yuv420p"],
        #     fps=config["fps"],
        # )

    def build_font_cache(self):
        from render_assets import FontCache

        return FontCache()

    def measure_prepare_step(self, prepare_trace, step_name, callback):
        if prepare_trace is None:
            return callback()

        started_at = datetime.now()
        result = callback()
        ended_at = datetime.now()
        prepare_trace.add_event(step_name, started_at, ended_at)
        return result

    def clip_dirty_box(self, left, top, right, bottom, width, height):
        clipped_left = max(0, int(math.floor(left)))
        clipped_top = max(0, int(math.floor(top)))
        clipped_right = min(width, int(math.ceil(right)))
        clipped_bottom = min(height, int(math.ceil(bottom)))
        if clipped_right <= clipped_left or clipped_bottom <= clipped_top:
            return None
        return (clipped_left, clipped_top, clipped_right, clipped_bottom)

    def build_dirty_region(self, base_image, left, top, right, bottom, width, height):
        from render_assets import DirtyRegion

        box = self.clip_dirty_box(left, top, right, bottom, width, height)
        if box is None:
            return None
        return DirtyRegion(box=box, background=base_image.crop(box))

    def build_dirty_regions(self, base_image, boxes, width, height):
        return [
            region
            for region in (
                self.build_dirty_region(
                    base_image, left, top, right, bottom, width, height
                )
                for left, top, right, bottom in boxes
            )
            if region is not None
        ]

    def union_layer_bbox(self, *layers):
        bbox = None
        for layer in layers:
            if layer is None:
                continue
            layer_bbox = layer.getbbox()
            if layer_bbox is None:
                continue
            if bbox is None:
                bbox = layer_bbox
                continue
            bbox = (
                min(bbox[0], layer_bbox[0]),
                min(bbox[1], layer_bbox[1]),
                max(bbox[2], layer_bbox[2]),
                max(bbox[3], layer_bbox[3]),
            )
        return bbox

    def widget_marker_padding(self, marker_sprite):
        if marker_sprite is None:
            return (0, 0)
        marker_width, marker_height = marker_sprite.size
        return (marker_width, marker_height)

    def measure_text_block(self, draw, font, texts):
        min_left = 0
        min_top = 0
        max_right = 0
        max_bottom = 0
        for text in texts:
            if not text:
                continue
            left, top, right, bottom = draw.multiline_textbbox((0, 0), text, font=font)
            min_left = min(min_left, left)
            min_top = min(min_top, top)
            max_right = max(max_right, right)
            max_bottom = max(max_bottom, bottom)
        return (min_left, min_top, max_right, max_bottom)

    def build_widget_content_dirty_region(
        self,
        base_image,
        width,
        height,
        widget_x,
        widget_y,
        content_bbox,
        marker_sprite,
    ):
        if content_bbox is None:
            return None

        pad_x, pad_y = self.widget_marker_padding(marker_sprite)
        return self.build_dirty_region(
            base_image,
            widget_x + content_bbox[0] - pad_x,
            widget_y + content_bbox[1] - pad_y,
            widget_x + content_bbox[2] + pad_x,
            widget_y + content_bbox[3] + pad_y,
            width,
            height,
        )

    def build_route_content_dirty_region(self, base_image, width, height, route_cache):
        if route_cache is None:
            return None

        content_bbox = self.union_layer_bbox(
            route_cache.rotated_background_layer or route_cache.background_layer,
            route_cache.rotated_completed_layer or route_cache.completed_layer,
        )
        return self.build_widget_content_dirty_region(
            base_image,
            width,
            height,
            route_cache.widget_x,
            route_cache.widget_y,
            content_bbox,
            route_cache.marker_sprite,
        )

    def build_elevation_label_dirty_region(
        self,
        base_image,
        width,
        height,
        font_cache,
        elevation_cache,
    ):
        from PIL import Image, ImageDraw

        from frame import Frame

        if elevation_cache is None:
            return None

        background_layer = (
            elevation_cache.rotated_background_layer or elevation_cache.background_layer
        )
        if background_layer is None:
            return None

        label_style = elevation_cache.label_style
        if label_style is None:
            return None

        label_texts = {
            frame_state.label_text
            for frame_state in elevation_cache.frame_states
            if frame_state.label_text
        }
        if not label_texts:
            return None

        measure_image = Image.new("RGBA", (1, 1))
        draw = ImageDraw.Draw(measure_image)
        font = Frame("", width, height, 0, 0).get_cached_font(
            font_cache,
            label_style.font_path,
            label_style.font_size,
        )
        text_left, text_top, text_right, text_bottom = self.measure_text_block(
            draw, font, label_texts
        )
        padding = 4
        background_width, background_height = background_layer.size

        left = elevation_cache.widget_x + label_style.x_offset + text_left - padding
        top = elevation_cache.widget_y + label_style.y_offset + text_top - padding
        right = (
            elevation_cache.widget_x
            + background_width
            + label_style.x_offset
            + text_right
            + padding
        )
        bottom = (
            elevation_cache.widget_y
            + background_height
            + label_style.y_offset
            + text_bottom
            + padding
        )

        return self.build_dirty_region(
            base_image, left, top, right, bottom, width, height
        )

    def build_elevation_content_dirty_region(
        self,
        base_image,
        width,
        height,
        elevation_cache,
    ):
        if elevation_cache is None:
            return None

        content_bbox = self.union_layer_bbox(
            elevation_cache.rotated_background_layer
            or elevation_cache.background_layer,
            elevation_cache.rotated_completed_layer or elevation_cache.completed_layer,
        )
        return self.build_widget_content_dirty_region(
            base_image,
            width,
            height,
            elevation_cache.widget_x,
            elevation_cache.widget_y,
            content_bbox,
            elevation_cache.marker_sprite,
        )

    def format_dynamic_value_text(self, attribute, value, config, scene_config):
        decimal_rounding = config.get("decimal_rounding")
        if decimal_rounding is None:
            decimal_rounding = scene_config.get("decimal_rounding")

        if "unit" in config:
            unit = config["unit"]
            if attribute == constant.ATTR_SPEED:
                if unit == constant.UNIT_IMPERIAL:
                    value *= constant.MPH_CONVERSION
                elif unit == constant.UNIT_METRIC:
                    value *= constant.KMH_CONVERSION
            elif attribute == constant.ATTR_ELEVATION:
                if unit == constant.UNIT_IMPERIAL:
                    value *= constant.FT_CONVERSION

        if (
            attribute == constant.ATTR_TIME
            and "hours_offset" in config
            and "time_format" in config
        ):
            hours_offset = config.get("hours_offset", 0)
            time_format = config.get(
                "time_format", scene_config.get("time_format", "%H:%M")
            )
            try:
                time_value = (
                    value
                    if isinstance(value, datetime)
                    else datetime.fromisoformat(str(value))
                )
                time_value = time_value + timedelta(hours=hours_offset)
                return time_value.strftime(time_format)
            except ValueError:
                return str(value)

        if isinstance(value, (int, float)) and decimal_rounding is not None:
            if decimal_rounding == 0:
                value = int(value)
            else:
                value = round(float(value), decimal_rounding)

        value = str(value)
        if "suffix" in config:
            value += config["suffix"]
        return value

    def build_dynamic_value_dirty_regions(
        self, base_image, width, height, render_assets
    ):
        from PIL import Image, ImageDraw

        from frame import Frame

        scene_config = self.template.get("scene", {})
        value_configs = self.template.get("values", [])
        if not value_configs:
            return []

        measure_image = Image.new("RGBA", (1, 1))
        draw = ImageDraw.Draw(measure_image)
        frame = Frame("", width, height, 0, 0)
        dirty_regions = []

        for config in value_configs:
            attribute = config.get("value")
            if attribute not in self.activity.valid_attributes:
                continue

            values = getattr(self.activity, attribute, [])
            if attribute == constant.ATTR_TIME:
                values = getattr(self.activity, "frame_timestamps", values)
            if not values:
                continue

            rendered_texts = {
                self.format_dynamic_value_text(attribute, value, config, scene_config)
                for value in values
            }
            if not rendered_texts:
                continue

            font_name = config.get("font", scene_config.get("font", "Arial.ttf"))
            font_size = config.get("font_size", scene_config.get("font_size", 32))
            font = frame.get_cached_font(render_assets.font_cache, font_name, font_size)
            text_left, text_top, text_right, text_bottom = self.measure_text_block(
                draw, font, rendered_texts
            )
            padding = 4

            region = self.build_dirty_region(
                base_image,
                config["x"] + text_left - padding,
                config["y"] + text_top - padding,
                config["x"] + text_right + padding,
                config["y"] + text_bottom + padding,
                width,
                height,
            )
            if region is not None:
                dirty_regions.append(region)

        return dirty_regions

    def get_plot_config(self, attribute):
        for config in self.template.get("plots", []):
            if config.get("value") == attribute:
                return config
        return None

    def simplify_polyline(self, points, tolerance):
        if len(points) <= 2:
            return list(points)

        def perpendicular_distance(point, start, end):
            x0, y0 = point
            x1, y1 = start
            x2, y2 = end
            dx = x2 - x1
            dy = y2 - y1
            if dx == 0 and dy == 0:
                return math.dist(point, start)
            return abs(dy * x0 - dx * y0 + x2 * y1 - y2 * x1) / math.sqrt(
                dx * dx + dy * dy
            )

        max_distance = 0.0
        split_index = 0
        for index in range(1, len(points) - 1):
            distance = perpendicular_distance(points[index], points[0], points[-1])
            if distance > max_distance:
                max_distance = distance
                split_index = index

        if max_distance <= tolerance:
            return [points[0], points[-1]]

        left = self.simplify_polyline(points[: split_index + 1], tolerance)
        right = self.simplify_polyline(points[split_index:], tolerance)
        return left[:-1] + right

    def cumulative_progress_for_points(self, points):
        if not points:
            return []
        cumulative_distances = [0.0]
        total_distance = 0.0
        for index in range(1, len(points)):
            total_distance += math.dist(points[index - 1], points[index])
            cumulative_distances.append(total_distance)
        if total_distance == 0:
            return [0.0 for _ in cumulative_distances]
        return [distance / total_distance for distance in cumulative_distances]

    def get_numeric_render_setting(self, config, key, default_value, cast_type=float):
        value = config.get(key, self.template.get("scene", {}).get(key, default_value))
        try:
            return cast_type(value)
        except (TypeError, ValueError):
            return default_value

    def fit_points_to_widget(self, points, width, height, margin=0.0, invert_y=False):
        if not points:
            return []

        min_x = min(point[0] for point in points)
        max_x = max(point[0] for point in points)
        min_y = min(point[1] for point in points)
        max_y = max(point[1] for point in points)

        inner_width = max(1.0, width * (1.0 - 2.0 * margin))
        inner_height = max(1.0, height * (1.0 - 2.0 * margin))
        span_x = max(max_x - min_x, 1e-9)
        span_y = max(max_y - min_y, 1e-9)
        scale = min(inner_width / span_x, inner_height / span_y)

        offset_x = (width - span_x * scale) / 2.0
        offset_y = (height - span_y * scale) / 2.0

        fitted = []
        for x_value, y_value in points:
            fitted_x = (x_value - min_x) * scale + offset_x
            fitted_y = (y_value - min_y) * scale + offset_y
            if invert_y:
                fitted_y = height - fitted_y
            fitted.append((fitted_x, fitted_y))
        return fitted

    def project_course_points(self, config):
        course_points = getattr(self.activity, "sample_course_points", None)
        if course_points is None:
            course_points = getattr(self.activity, constant.ATTR_COURSE, [])
        if not course_points:
            return []

        latitudes = [point[0] for point in course_points]
        mean_latitude = math.radians(sum(latitudes) / len(latitudes))
        projected = []
        for latitude, longitude in course_points:
            x_value = longitude * math.cos(mean_latitude)
            y_value = latitude
            projected.append((x_value, y_value))

        return self.fit_points_to_widget(
            projected,
            config["width"],
            config["height"],
            margin=config.get("margin", 0.0),
            invert_y=True,
        )

    def downsample_elevation_points(self, points, target_count):
        if len(points) <= target_count:
            return list(points)

        bucket_size = len(points) / max(target_count // 2, 1)
        sampled = [points[0]]
        bucket_index = 0.0
        while bucket_index < len(points) - 1:
            start_index = int(bucket_index)
            end_index = min(len(points), int(bucket_index + bucket_size))
            bucket = points[start_index:end_index]
            if bucket:
                sampled.append(min(bucket, key=lambda point: point[1]))
                sampled.append(max(bucket, key=lambda point: point[1]))
            bucket_index += bucket_size

        sampled.append(points[-1])
        sampled = sorted(set(sampled), key=lambda point: point[0])
        return sampled

    def get_elevation_raw_points(self):
        elevations = getattr(self.activity, "sample_elevations", None)
        if elevations is None:
            elevations = getattr(self.activity, constant.ATTR_ELEVATION, [])
        if not elevations:
            return []

        distance_progress = getattr(self.activity, "sample_distance_progress", [])
        if distance_progress and len(distance_progress) == len(elevations):
            return list(zip(distance_progress, elevations))

        last_index = max(len(elevations) - 1, 1)
        return [
            (index / last_index, elevation)
            for index, elevation in enumerate(elevations)
        ]

    def project_elevation_points(self, config):
        raw_points = self.get_elevation_raw_points()
        if not raw_points:
            return []

        downsample_multiplier = self.get_numeric_render_setting(
            config,
            "elevation_downsample_multiplier",
            constant.DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER,
        )
        downsample_multiplier = max(0.1, downsample_multiplier)
        target_count = max(
            2,
            min(len(raw_points), int(config["width"] * downsample_multiplier)),
        )
        downsampled_points = self.downsample_elevation_points(raw_points, target_count)

        margin = config.get("margin", 0.0)
        inner_width = max(1.0, config["width"] * (1.0 - 2.0 * margin))
        inner_height = max(1.0, config["height"] * (1.0 - 2.0 * margin))

        min_elevation = min(point[1] for point in downsampled_points)
        max_elevation = max(point[1] for point in downsampled_points)
        elevation_span = max(max_elevation - min_elevation, 1e-9)
        last_progress = max(downsampled_points[-1][0], 1e-9)

        normalized_points = []
        for sample_progress, elevation in downsampled_points:
            progress01 = sample_progress / last_progress
            normalized_elevation = (elevation - min_elevation) / elevation_span
            point_x = config["width"] * margin + inner_width * progress01
            point_y = config["height"] - (
                config["height"] * margin + inner_height * normalized_elevation
            )
            normalized_points.append((point_x, point_y))

        return normalized_points

    def build_elevation_debug_payload(self, config, geometry):
        raw_points = self.get_elevation_raw_points()
        downsample_multiplier = self.get_numeric_render_setting(
            config,
            "elevation_downsample_multiplier",
            constant.DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER,
        )
        downsample_multiplier = max(0.1, downsample_multiplier)
        target_count = (
            max(2, min(len(raw_points), int(config["width"] * downsample_multiplier)))
            if raw_points
            else 0
        )
        downsampled_points = (
            self.downsample_elevation_points(raw_points, target_count)
            if raw_points
            else []
        )

        return {
            "widget": {
                "width": config["width"],
                "height": config["height"],
                "margin": config.get("margin", 0.0),
                "rotation": config.get("rotation", 0),
                "elevation_downsample_multiplier": downsample_multiplier,
            },
            "raw": {
                "count": len(raw_points),
                "min_elevation": min((point[1] for point in raw_points), default=None),
                "max_elevation": max((point[1] for point in raw_points), default=None),
                "points": raw_points,
            },
            "simplified_source": {
                "target_count": target_count,
                "count": len(downsampled_points),
                "points": downsampled_points,
            },
            "normalized_geometry": {
                "count": len(geometry.points),
                "points": geometry.points,
                "cumulative_progress": geometry.cumulative_progress,
            },
        }

    def build_route_geometry(self, config):
        from render_assets import WidgetGeometry

        raw_points = self.project_course_points(config)
        tolerance = self.get_numeric_render_setting(
            config,
            "route_simplify_tolerance_px",
            constant.DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX,
        )
        tolerance_multiplier = self.get_numeric_render_setting(
            config,
            "route_simplify_tolerance_multiplier",
            constant.DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER,
        )
        tolerance = max(0.05, tolerance * max(0.1, tolerance_multiplier))
        simplified_points = self.simplify_polyline(raw_points, tolerance)
        bbox = (0.0, 0.0, float(config["width"]), float(config["height"]))
        return WidgetGeometry(
            points=simplified_points,
            bbox=bbox,
            cumulative_progress=self.cumulative_progress_for_points(simplified_points),
        )

    def build_elevation_geometry(self, config):
        from render_assets import WidgetGeometry

        simplified_points = self.project_elevation_points(config)
        bbox = (0.0, 0.0, float(config["width"]), float(config["height"]))
        return WidgetGeometry(
            points=simplified_points,
            bbox=bbox,
            cumulative_progress=self.cumulative_progress_for_points(simplified_points),
        )

    def total_frame_count(self, seconds):
        interpolated_frames = self.interpolated_frame_count()
        if interpolated_frames > 0:
            return interpolated_frames
        if seconds is None:
            return 0
        return max(0, round(seconds * self.fps))

    def route_reveal_bucket_count(self, config):
        configured_count = self.get_numeric_render_setting(
            config,
            "route_reveal_bucket_count",
            128,
            cast_type=int,
        )
        configured_count = max(2, configured_count)
        interpolated_frames = self.interpolated_frame_count()
        if interpolated_frames <= 0:
            return configured_count
        return max(2, min(interpolated_frames, configured_count))

    def interpolated_frame_count(self):
        frame_elapsed_seconds = getattr(self.activity, "frame_elapsed_seconds", [])
        if frame_elapsed_seconds:
            return len(frame_elapsed_seconds)
        for attribute in self.activity.valid_attributes:
            if attribute in constant.NO_INTERPOLATE_ATTRIBUTES:
                continue
            return len(getattr(self.activity, attribute))
        return 0

    def frame_time_components(self, frame_index):
        return frame_index // self.fps, frame_index % self.fps

    def frame_activity_index(self, frame_index, last_index):
        return min(frame_index, last_index)

    def elevation_label_text(self, elevation_value, config):
        text = ""
        for unit in config["units"]:
            value = elevation_value * constant.ELEVATION_CONVERSION_MAP[unit]
            if "decimal_rounding" in config.keys():
                if config["decimal_rounding"] == 0:
                    value = int(value)
                else:
                    value = round(float(value), config["decimal_rounding"])
            text += (
                f"{value}{constant.DEFAULT_SUFFIX_MAP[constant.ATTR_ELEVATION][unit]}\n"
            )
        return text.rstrip()

    def build_route_frame_states(self, route_cache):
        from render_assets import RouteFrameState

        if route_cache is None:
            return []

        if route_cache.geometry is None or not route_cache.geometry.points:
            return []

        frame_progress = getattr(self.activity, "frame_distance_progress", [])
        total_frames = self.interpolated_frame_count()
        frame_states = []
        for frame_index in range(total_frames):
            if frame_progress:
                progress01 = frame_progress[min(frame_index, len(frame_progress) - 1)]
            else:
                progress01 = frame_index / max(total_frames - 1, 1)
            segment_index, marker_x, marker_y = self.route_position_at_progress(
                route_cache.geometry.points,
                route_cache.cumulative_progress,
                progress01,
            )
            if route_cache.rotation_deg != 0:
                marker_x, marker_y = self.transform_rotated_point(
                    marker_x,
                    marker_y,
                    route_cache.widget_width,
                    route_cache.widget_height,
                    route_cache.rotation_deg,
                )
            frame_states.append(
                RouteFrameState(
                    progress01=progress01,
                    marker_x=marker_x,
                    marker_y=marker_y,
                    segment_index=segment_index,
                    bucket_index=min(
                        route_cache.bucket_count - 1,
                        round(progress01 * max(route_cache.bucket_count - 1, 1)),
                    ),
                )
            )
        return frame_states

    def build_elevation_frame_states(self, elevation_cache):
        from render_assets import ElevationFrameState

        if elevation_cache is None:
            return []

        elevations = getattr(self.activity, constant.ATTR_ELEVATION, [])
        if not elevations:
            return []

        config = elevation_cache.source_config
        min_elevation = min(elevations)
        max_elevation = max(elevations)
        span = max(max_elevation - min_elevation, 1e-9)
        margin = config.get("margin", 0.0)
        inner_width = config["width"] * (1.0 - 2.0 * margin)
        inner_height = config["height"] * (1.0 - 2.0 * margin)
        total_frames = self.interpolated_frame_count()
        last_index = max(len(elevations) - 1, 1)
        frame_progress = getattr(self.activity, "frame_distance_progress", [])

        frame_states = []
        for frame_index in range(total_frames):
            activity_index = self.frame_activity_index(frame_index, last_index)
            if frame_progress:
                progress01 = frame_progress[min(frame_index, len(frame_progress) - 1)]
            else:
                progress01 = frame_index / max(total_frames - 1, 1)
            elevation_value = elevations[activity_index]
            marker_x = config["width"] * margin + inner_width * progress01
            normalized_elevation = (elevation_value - min_elevation) / span
            marker_y = config["height"] - (
                config["height"] * margin + inner_height * normalized_elevation
            )
            label_text = (
                self.elevation_label_text(elevation_value, config["point_label"])
                if "point_label" in config
                else None
            )
            frame_states.append(
                ElevationFrameState(
                    progress01=progress01,
                    marker_x=marker_x,
                    marker_y=marker_y,
                    elevation_m=elevation_value,
                    label_text=label_text,
                )
            )
        return frame_states

    def color_with_scaled_opacity(
        self, color, opacity_scale=1.0, opacity_override=None
    ):
        from PIL import ImageColor

        rgba = list(ImageColor.getcolor(color, "RGBA"))
        base_alpha = rgba[3] / 255.0
        target_alpha = opacity_override if opacity_override is not None else base_alpha
        rgba[3] = max(0, min(255, round(target_alpha * opacity_scale * 255)))
        return tuple(rgba)

    def rotate_static_layer(self, layer, rotation_deg):
        if layer is None or rotation_deg == 0:
            return None
        return layer.rotate(rotation_deg, resample=3, expand=True)

    def transform_rotated_point(self, x_value, y_value, width, height, rotation_deg):
        angle_rad = math.radians(-rotation_deg)
        center_x = width / 2.0
        center_y = height / 2.0
        translated_x = x_value - center_x
        translated_y = y_value - center_y

        rotated_x = translated_x * math.cos(angle_rad) - translated_y * math.sin(
            angle_rad
        )
        rotated_y = translated_x * math.sin(angle_rad) + translated_y * math.cos(
            angle_rad
        )

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

    def render_antialiased_polyline(
        self,
        widget_width,
        widget_height,
        points,
        line_width,
        color,
        opacity_scale,
        supersample_scale,
    ):
        from PIL import Image, ImageDraw

        if len(points) < 2:
            return Image.new("RGBA", (widget_width, widget_height), (0, 0, 0, 0))

        scale = max(1, int(round(supersample_scale)))
        if scale == 1:
            image = Image.new("RGBA", (widget_width, widget_height), (0, 0, 0, 0))
            ImageDraw.Draw(image).line(
                points,
                fill=self.color_with_scaled_opacity(color, opacity_scale=opacity_scale),
                width=max(1, round(line_width)),
                joint="curve",
            )
            return image

        scaled_image = Image.new(
            "RGBA",
            (widget_width * scale, widget_height * scale),
            (0, 0, 0, 0),
        )
        scaled_points = [
            (x_value * scale, y_value * scale) for x_value, y_value in points
        ]
        ImageDraw.Draw(scaled_image).line(
            scaled_points,
            fill=self.color_with_scaled_opacity(color, opacity_scale=opacity_scale),
            width=max(1, round(line_width * scale)),
            joint="curve",
        )
        return scaled_image.resize((widget_width, widget_height), resample=1)

    def build_marker_sprite(self, point_configs, default_color, scale=1.0):
        from PIL import Image, ImageDraw

        if not point_configs:
            point_configs = [{"weight": constant.DEFAULT_POINT_WEIGHT}]

        layers = []
        for point_config in point_configs:
            radius = max(
                2,
                round(
                    math.sqrt(max(get_point_weight(point_config), 1)) * max(scale, 0.1)
                ),
            )
            layers.append((radius, point_config))

        layers.sort(key=lambda layer: layer[0], reverse=True)
        radii = [radius for radius, _ in layers]

        max_radius = max(radii)
        sprite_size = max_radius * 2 + 8
        center = sprite_size // 2
        sprite = Image.new("RGBA", (sprite_size, sprite_size), (0, 0, 0, 0))
        draw = ImageDraw.Draw(sprite)

        for index, (radius, point_config) in enumerate(layers):
            color = (
                get_point_color(point_config)
                if "color" in point_config
                else default_color
            )
            opacity = point_config.get("opacity", 1.0)
            bounds = (
                center - radius,
                center - radius,
                center + radius,
                center + radius,
            )

            # Render the largest layers as thin rings; only the smallest layer is solid.
            if index < len(layers) - 1:
                ring_width = max(1, min(3, round(radius * 0.18)))
                outline = self.color_with_scaled_opacity(color, opacity_scale=opacity)
                draw.ellipse(bounds, outline=outline, width=ring_width)
            else:
                fill = self.color_with_scaled_opacity(color, opacity_scale=opacity)
                draw.ellipse(bounds, fill=fill)

        return sprite, (center, center)

    def build_route_layers(self, route_cache):
        if (
            route_cache is None
            or not route_cache.geometry
            or len(route_cache.geometry.points) < 2
        ):
            return None, None

        config = route_cache.source_config
        line_color = get_line_color(config)
        line_width = max(
            1,
            round(
                get_line_width(config) * constant.DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER
            ),
        )
        background = None
        if constant.ENABLE_ROUTE_BACKGROUND_LAYER:
            background = self.render_antialiased_polyline(
                route_cache.widget_width,
                route_cache.widget_height,
                route_cache.geometry.points,
                line_width,
                line_color,
                constant.DEFAULT_ROUTE_BACKGROUND_OPACITY_SCALE,
                constant.DEFAULT_ROUTE_LAYER_SUPERSAMPLE,
            )
        completed = self.render_antialiased_polyline(
            route_cache.widget_width,
            route_cache.widget_height,
            route_cache.geometry.points,
            line_width,
            line_color,
            1.0,
            constant.DEFAULT_ROUTE_LAYER_SUPERSAMPLE,
        )
        return background, completed

    def route_points_for_progress(self, points, cumulative_progress, progress_limit):
        if not points:
            return []
        if progress_limit <= 0:
            return [points[0]]
        if progress_limit >= 1 or len(points) == 1:
            return list(points)

        prefix_points = [points[0]]
        for index in range(1, len(points)):
            start_progress = cumulative_progress[index - 1]
            end_progress = cumulative_progress[index]
            end_point = points[index]

            if progress_limit >= end_progress:
                prefix_points.append(end_point)
                continue

            segment_span = max(end_progress - start_progress, 1e-9)
            ratio = (progress_limit - start_progress) / segment_span
            start_x, start_y = points[index - 1]
            end_x, end_y = end_point
            interp_x = start_x + (end_x - start_x) * ratio
            interp_y = start_y + (end_y - start_y) * ratio
            prefix_points.append((interp_x, interp_y))
            break

        return prefix_points

    def route_point_at_progress(self, points, cumulative_progress, progress_limit):
        _, marker_x, marker_y = self.route_position_at_progress(
            points,
            cumulative_progress,
            progress_limit,
        )
        return marker_x, marker_y

    def route_position_at_progress(self, points, cumulative_progress, progress_limit):
        if not points:
            return 0, 0.0, 0.0
        if len(points) == 1:
            return 0, points[0][0], points[0][1]
        if progress_limit <= 0:
            return 1, points[0][0], points[0][1]
        if progress_limit >= 1:
            last_index = len(points) - 1
            return last_index, points[-1][0], points[-1][1]

        for index in range(1, len(points)):
            start_progress = cumulative_progress[index - 1]
            end_progress = cumulative_progress[index]
            end_point = points[index]

            if progress_limit >= end_progress:
                continue

            segment_span = max(end_progress - start_progress, 1e-9)
            ratio = (progress_limit - start_progress) / segment_span
            start_x, start_y = points[index - 1]
            end_x, end_y = end_point
            interp_x = start_x + (end_x - start_x) * ratio
            interp_y = start_y + (end_y - start_y) * ratio
            return index, interp_x, interp_y

        last_index = len(points) - 1
        return last_index, points[-1][0], points[-1][1]

    def build_route_bucket_assets(self, route_cache, representative_only=False):
        from PIL import Image

        if (
            route_cache is None
            or not route_cache.geometry
            or len(route_cache.geometry.points) < 2
        ):
            return None

        bucket_indices = range(route_cache.bucket_count)
        if representative_only:
            sample_indices = {
                0,
                route_cache.bucket_count // 4,
                route_cache.bucket_count // 2,
                (route_cache.bucket_count * 3) // 4,
                route_cache.bucket_count - 1,
            }
            bucket_indices = sorted(
                index
                for index in sample_indices
                if 0 <= index < route_cache.bucket_count
            )

        overlays = {}
        line_color = get_line_color(route_cache.source_config)
        line_width = max(
            1,
            round(
                get_line_width(route_cache.source_config)
                * constant.DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER
            ),
        )

        for bucket_index in bucket_indices:
            progress_limit = bucket_index / max(route_cache.bucket_count - 1, 1)
            overlay = Image.new(
                "RGBA",
                (route_cache.widget_width, route_cache.widget_height),
                (0, 0, 0, 0),
            )
            points = self.route_points_for_progress(
                route_cache.geometry.points,
                route_cache.geometry.cumulative_progress,
                progress_limit,
            )
            if len(points) >= 2:
                overlay = self.render_antialiased_polyline(
                    route_cache.widget_width,
                    route_cache.widget_height,
                    points,
                    line_width,
                    line_color,
                    1.0,
                    constant.DEFAULT_ROUTE_LAYER_SUPERSAMPLE,
                )
            if route_cache.rotation_deg != 0:
                overlay = self.rotate_static_layer(overlay, route_cache.rotation_deg)
            overlays[bucket_index] = overlay

        if representative_only:
            return overlays

        return [overlays[index] for index in range(route_cache.bucket_count)]

    def build_route_bucket_masks(self, route_cache, representative_only=False):
        from PIL import Image, ImageDraw

        if (
            route_cache is None
            or not route_cache.geometry
            or len(route_cache.display_points) < 2
        ):
            return None

        route_layer_for_mask = (
            route_cache.rotated_completed_layer or route_cache.completed_layer
        )
        if route_layer_for_mask is None:
            return None

        bucket_indices = range(route_cache.bucket_count)
        if representative_only:
            sample_indices = {
                0,
                route_cache.bucket_count // 4,
                route_cache.bucket_count // 2,
                (route_cache.bucket_count * 3) // 4,
                route_cache.bucket_count - 1,
            }
            bucket_indices = sorted(
                index
                for index in sample_indices
                if 0 <= index < route_cache.bucket_count
            )

        masks = {}
        for bucket_index in bucket_indices:
            progress_limit = bucket_index / max(route_cache.bucket_count - 1, 1)
            mask = Image.new("L", route_layer_for_mask.size, 0)
            points = self.route_points_for_progress(
                route_cache.display_points,
                route_cache.geometry.cumulative_progress,
                progress_limit,
            )
            if len(points) >= 2:
                ImageDraw.Draw(mask).line(
                    points,
                    fill=255,
                    width=max(1, round(route_cache.line_width)),
                    joint="curve",
                )
            masks[bucket_index] = mask

        if representative_only:
            return masks

        return [masks[index] for index in range(route_cache.bucket_count)]

    def build_elevation_layers(self, elevation_cache):
        from PIL import Image, ImageDraw

        if (
            elevation_cache is None
            or not elevation_cache.geometry
            or len(elevation_cache.geometry.points) < 2
        ):
            return None, None

        config = elevation_cache.source_config
        line_color = get_line_color(config)
        line_width = max(
            1,
            round(
                get_line_width(config)
                * constant.DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER
            ),
        )
        fill_opacity = get_opacity(config.get("fill", {})) if "fill" in config else 0.0

        background = Image.new(
            "RGBA",
            (elevation_cache.widget_width, elevation_cache.widget_height),
            (0, 0, 0, 0),
        )
        completed = Image.new(
            "RGBA",
            (elevation_cache.widget_width, elevation_cache.widget_height),
            (0, 0, 0, 0),
        )
        background_draw = ImageDraw.Draw(background)
        completed_draw = ImageDraw.Draw(completed)

        profile_points = elevation_cache.geometry.points
        baseline_y = elevation_cache.widget_height
        polygon_points = [
            (profile_points[0][0], baseline_y),
            *profile_points,
            (profile_points[-1][0], baseline_y),
        ]

        if fill_opacity > 0:
            background_draw.polygon(
                polygon_points,
                fill=self.color_with_scaled_opacity(
                    line_color,
                    opacity_scale=0.35,
                    opacity_override=fill_opacity,
                ),
            )
            completed_draw.polygon(
                polygon_points,
                fill=self.color_with_scaled_opacity(
                    line_color,
                    opacity_scale=1.0,
                    opacity_override=fill_opacity,
                ),
            )

        background_draw.line(
            profile_points,
            fill=self.color_with_scaled_opacity(line_color, opacity_scale=1.0),
            width=line_width,
            joint="curve",
        )
        return background, completed

    def save_representative_elevation_reveals(self, elevation_cache, render_debug):
        if (
            elevation_cache is None
            or elevation_cache.background_layer is None
            or elevation_cache.completed_layer is None
            or render_debug is None
        ):
            return

        sample_points = [0.0, 0.25, 0.5, 0.75, 1.0]
        for progress01 in sample_points:
            reveal = elevation_cache.background_layer.copy()
            reveal_width = max(
                1,
                min(
                    elevation_cache.completed_layer.width,
                    round(elevation_cache.completed_layer.width * progress01),
                ),
            )
            completed_crop = elevation_cache.completed_layer.crop(
                (0, 0, reveal_width, elevation_cache.completed_layer.height)
            )
            reveal.paste(completed_crop, (0, 0), completed_crop)
            render_debug.save_image(
                f"elevation_reveal_{int(progress01 * 100):03d}.png",
                reveal,
            )

    def build_route_cache(self, prepare_trace=None):
        from render_assets import RouteWidgetCache

        config = self.get_plot_config(constant.ATTR_COURSE)
        if not config:
            return None

        geometry = self.measure_prepare_step(
            prepare_trace,
            "build_route_cache.geometry",
            lambda: self.build_route_geometry(config),
        )
        route_cache = RouteWidgetCache(
            source_config=config,
            geometry=geometry,
            widget_x=config["x"],
            widget_y=config["y"],
            widget_width=config["width"],
            widget_height=config["height"],
            rotation_deg=config.get("rotation", 0),
            render_mode="bucket_mask",
            bucket_count=self.route_reveal_bucket_count(config),
            background_layer=None,
            completed_layer=None,
            rotated_background_layer=None,
            rotated_completed_layer=None,
            marker_sprite=None,
            marker_anchor=(0, 0),
            line_width=max(
                1,
                round(
                    get_line_width(config)
                    * constant.DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER
                ),
            ),
            simplified_points=geometry.points,
            cumulative_progress=geometry.cumulative_progress,
            display_points=geometry.points,
            bucket_masks=None,
            bucket_overlays=None,
        )
        route_cache.background_layer, route_cache.completed_layer = (
            self.measure_prepare_step(
                prepare_trace,
                "build_route_cache.layers",
                lambda: self.build_route_layers(route_cache),
            )
        )
        route_cache.rotated_background_layer = self.measure_prepare_step(
            prepare_trace,
            "build_route_cache.rotated_background_layer",
            lambda: self.rotate_static_layer(
                route_cache.background_layer,
                route_cache.rotation_deg,
            ),
        )
        route_cache.rotated_completed_layer = self.measure_prepare_step(
            prepare_trace,
            "build_route_cache.rotated_completed_layer",
            lambda: self.rotate_static_layer(
                route_cache.completed_layer,
                route_cache.rotation_deg,
            ),
        )
        route_cache.display_points = self.measure_prepare_step(
            prepare_trace,
            "build_route_cache.display_points",
            lambda: [
                self.transform_rotated_point(
                    point_x,
                    point_y,
                    route_cache.widget_width,
                    route_cache.widget_height,
                    route_cache.rotation_deg,
                )
                for point_x, point_y in route_cache.geometry.points
            ]
            if route_cache.rotation_deg != 0
            else list(route_cache.geometry.points),
        )
        route_cache.marker_sprite, route_cache.marker_anchor = (
            self.measure_prepare_step(
                prepare_trace,
                "build_route_cache.marker_sprite",
                lambda: self.build_marker_sprite(
                    config.get("points", []),
                    get_line_color(config),
                ),
            )
        )
        route_cache.bucket_masks = self.measure_prepare_step(
            prepare_trace,
            "build_route_cache.bucket_masks",
            lambda: self.build_route_bucket_masks(route_cache),
        )
        route_cache.frame_states = self.measure_prepare_step(
            prepare_trace,
            "build_route_cache.frame_states",
            lambda: self.build_route_frame_states(route_cache),
        )
        return route_cache

    def build_elevation_cache(self, prepare_trace=None):
        from render_assets import ElevationLabelStyle, ElevationWidgetCache

        config = self.get_plot_config(constant.ATTR_ELEVATION)
        if not config:
            return None

        geometry = self.measure_prepare_step(
            prepare_trace,
            "build_elevation_cache.geometry",
            lambda: self.build_elevation_geometry(config),
        )
        label_style = None
        point_label = config.get("point_label")
        if point_label:
            label_style = ElevationLabelStyle(
                font_path=point_label.get(
                    "font", self.template["scene"].get("font", "Arial.ttf")
                ),
                font_size=point_label.get(
                    "font_size", self.template["scene"].get("font_size", 32)
                ),
                color=point_label.get(
                    "color", config.get("color", constant.DEFAULT_COLOR)
                ),
                x_offset=point_label.get("x_offset", 0),
                y_offset=point_label.get("y_offset", 0),
                units=point_label.get("units", [constant.UNIT_METRIC]),
                decimal_rounding=point_label.get("decimal_rounding"),
            )

        elevation_cache = ElevationWidgetCache(
            source_config=config,
            geometry=geometry,
            widget_x=config["x"],
            widget_y=config["y"],
            widget_width=config["width"],
            widget_height=config["height"],
            rotation_deg=config.get("rotation", 0),
            background_layer=None,
            completed_layer=None,
            rotated_background_layer=None,
            rotated_completed_layer=None,
            marker_sprite=None,
            marker_anchor=(0, 0),
            simplified_points=geometry.points,
            label_style=label_style,
        )
        elevation_cache.background_layer, elevation_cache.completed_layer = (
            self.measure_prepare_step(
                prepare_trace,
                "build_elevation_cache.layers",
                lambda: self.build_elevation_layers(elevation_cache),
            )
        )
        elevation_cache.rotated_background_layer = self.measure_prepare_step(
            prepare_trace,
            "build_elevation_cache.rotated_background_layer",
            lambda: self.rotate_static_layer(
                elevation_cache.background_layer,
                elevation_cache.rotation_deg,
            ),
        )
        elevation_cache.rotated_completed_layer = self.measure_prepare_step(
            prepare_trace,
            "build_elevation_cache.rotated_completed_layer",
            lambda: self.rotate_static_layer(
                elevation_cache.completed_layer,
                elevation_cache.rotation_deg,
            ),
        )
        elevation_cache.marker_sprite, elevation_cache.marker_anchor = (
            self.measure_prepare_step(
                prepare_trace,
                "build_elevation_cache.marker_sprite",
                lambda: self.build_marker_sprite(
                    config.get("points", []),
                    get_line_color(config),
                    scale=constant.DEFAULT_ELEVATION_MARKER_SCALE,
                ),
            )
        )
        elevation_cache.frame_states = self.measure_prepare_step(
            prepare_trace,
            "build_elevation_cache.frame_states",
            lambda: self.build_elevation_frame_states(elevation_cache),
        )
        return elevation_cache

    def save_geometry_preview(
        self, filename, widget_width, widget_height, points, color, render_debug
    ):
        from PIL import Image, ImageDraw

        if not render_debug:
            return

        image = Image.new("RGBA", (widget_width, widget_height), (0, 0, 0, 0))
        if len(points) >= 2:
            ImageDraw.Draw(image).line(points, fill=color, width=3)
        render_debug.save_image(filename, image)

    def prepare_render_assets(
        self,
        width=None,
        height=None,
        render_debug=None,
        render_profiler=None,
    ):
        from PIL import Image

        from render_debug import RenderPreparationTrace
        from render_assets import RenderAssets

        if width is None:
            width = self.template["scene"]["width"]
        if height is None:
            height = self.template["scene"]["height"]

        prepare_trace = RenderPreparationTrace(
            started_at_iso=datetime.now().isoformat(timespec="milliseconds")
        )

        prepare_started = datetime.now()
        render_assets = RenderAssets(
            font_cache=self.measure_prepare_step(
                prepare_trace,
                "build_font_cache",
                self.build_font_cache,
            )
        )
        render_assets.route_cache = self.measure_prepare_step(
            prepare_trace,
            "build_route_cache",
            lambda: self.build_route_cache(prepare_trace=prepare_trace),
        )
        render_assets.elevation_cache = self.measure_prepare_step(
            prepare_trace,
            "build_elevation_cache",
            lambda: self.build_elevation_cache(prepare_trace=prepare_trace),
        )

        # Pre-render static elements once. Phase 2 only moves this behind a seam.
        render_assets.base_image = self.measure_prepare_step(
            prepare_trace,
            "create_base_image",
            lambda: Image.new("RGBA", (width, height)),
        )

        if "labels" in self.template.keys():
            from frame import Frame

            static_frame = Frame("", width, height, 0, 0)
            for config in self.template["labels"]:
                if render_profiler:

                    def draw_static_label():
                        with render_profiler.measure("text.static.cache"):
                            return static_frame.draw_value(
                                render_assets.base_image,
                                config["text"],
                                config,
                                self.template.get("scene", {}),
                                font_cache=render_assets.font_cache,
                            )

                    render_assets.base_image = self.measure_prepare_step(
                        prepare_trace,
                        f"draw_static_label:{config['text']}",
                        draw_static_label,
                    )
                else:
                    render_assets.base_image = self.measure_prepare_step(
                        prepare_trace,
                        f"draw_static_label:{config['text']}",
                        lambda config=config: static_frame.draw_value(
                            render_assets.base_image,
                            config["text"],
                            config,
                            self.template.get("scene", {}),
                            font_cache=render_assets.font_cache,
                        ),
                    )

        for config in self.template.get("plots", []):
            attribute = config["value"]
            if attribute in {constant.ATTR_COURSE, constant.ATTR_ELEVATION}:
                continue
            raise ValueError(
                f"Legacy plot rendering is no longer supported for '{attribute}'."
            )

        render_assets.dirty_regions = []
        route_content_region = self.build_route_content_dirty_region(
            render_assets.base_image,
            width,
            height,
            render_assets.route_cache,
        )
        if route_content_region is not None:
            render_assets.dirty_regions.append(route_content_region)

        elevation_content_region = self.build_elevation_content_dirty_region(
            render_assets.base_image,
            width,
            height,
            render_assets.elevation_cache,
        )
        if elevation_content_region is not None:
            render_assets.dirty_regions.append(elevation_content_region)

        elevation_label_region = self.build_elevation_label_dirty_region(
            render_assets.base_image,
            width,
            height,
            render_assets.font_cache,
            render_assets.elevation_cache,
        )
        if elevation_label_region is not None:
            render_assets.dirty_regions.append(elevation_label_region)
        render_assets.dirty_regions.extend(
            self.build_dynamic_value_dirty_regions(
                render_assets.base_image,
                width,
                height,
                render_assets,
            )
        )

        prepare_trace.add_event(
            "prepare_render_assets.total", prepare_started, datetime.now()
        )

        if render_debug:
            render_debug.save_image("base_image.png", render_assets.base_image)
            for attribute, (plot_bg, _) in render_assets.plot_backgrounds.items():
                render_debug.save_image(f"plot_background_{attribute}.png", plot_bg)
            if render_assets.route_cache and render_assets.route_cache.geometry:
                self.save_geometry_preview(
                    "route_geometry.png",
                    render_assets.route_cache.widget_width,
                    render_assets.route_cache.widget_height,
                    render_assets.route_cache.geometry.points,
                    "#f4f4f4",
                    render_debug,
                )
                render_debug.save_json(
                    "route_frame_states.json",
                    {
                        "count": len(render_assets.route_cache.frame_states),
                        "sample": [
                            render_assets.route_cache.frame_states[index].__dict__
                            for index in [
                                0,
                                len(render_assets.route_cache.frame_states) // 4,
                                len(render_assets.route_cache.frame_states) // 2,
                                (len(render_assets.route_cache.frame_states) * 3) // 4,
                                len(render_assets.route_cache.frame_states) - 1,
                            ]
                            if 0 <= index < len(render_assets.route_cache.frame_states)
                        ],
                    },
                )
                if render_assets.route_cache.background_layer is not None:
                    render_debug.save_image(
                        "route_background.png",
                        render_assets.route_cache.background_layer,
                    )
                if render_assets.route_cache.completed_layer is not None:
                    render_debug.save_image(
                        "route_completed.png",
                        render_assets.route_cache.completed_layer,
                    )
                if render_assets.route_cache.rotated_background_layer is not None:
                    render_debug.save_image(
                        "route_background_rotated.png",
                        render_assets.route_cache.rotated_background_layer,
                    )
                if render_assets.route_cache.rotated_completed_layer is not None:
                    render_debug.save_image(
                        "route_completed_rotated.png",
                        render_assets.route_cache.rotated_completed_layer,
                    )
                if render_assets.route_cache.marker_sprite is not None:
                    render_debug.save_image(
                        "route_marker_sprite.png",
                        render_assets.route_cache.marker_sprite,
                    )
            if render_assets.elevation_cache and render_assets.elevation_cache.geometry:
                self.save_geometry_preview(
                    "elevation_geometry.png",
                    render_assets.elevation_cache.widget_width,
                    render_assets.elevation_cache.widget_height,
                    render_assets.elevation_cache.geometry.points,
                    "#f4f4f4",
                    render_debug,
                )
                render_debug.save_json(
                    "elevation_frame_states.json",
                    {
                        "count": len(render_assets.elevation_cache.frame_states),
                        "sample": [
                            render_assets.elevation_cache.frame_states[index].__dict__
                            for index in [
                                0,
                                len(render_assets.elevation_cache.frame_states) // 4,
                                len(render_assets.elevation_cache.frame_states) // 2,
                                (len(render_assets.elevation_cache.frame_states) * 3)
                                // 4,
                                len(render_assets.elevation_cache.frame_states) - 1,
                            ]
                            if 0
                            <= index
                            < len(render_assets.elevation_cache.frame_states)
                        ],
                    },
                )
                render_debug.save_json(
                    "elevation_geometry_data.json",
                    self.build_elevation_debug_payload(
                        render_assets.elevation_cache.source_config,
                        render_assets.elevation_cache.geometry,
                    ),
                )
                if render_assets.elevation_cache.background_layer is not None:
                    render_debug.save_image(
                        "elevation_background.png",
                        render_assets.elevation_cache.background_layer,
                    )
                if render_assets.elevation_cache.completed_layer is not None:
                    render_debug.save_image(
                        "elevation_completed.png",
                        render_assets.elevation_cache.completed_layer,
                    )
                if render_assets.elevation_cache.rotated_background_layer is not None:
                    render_debug.save_image(
                        "elevation_background_rotated.png",
                        render_assets.elevation_cache.rotated_background_layer,
                    )
                if render_assets.elevation_cache.rotated_completed_layer is not None:
                    render_debug.save_image(
                        "elevation_completed_rotated.png",
                        render_assets.elevation_cache.rotated_completed_layer,
                    )
                if render_assets.elevation_cache.marker_sprite is not None:
                    render_debug.save_image(
                        "elevation_marker_sprite.png",
                        render_assets.elevation_cache.marker_sprite,
                    )
                self.save_representative_elevation_reveals(
                    render_assets.elevation_cache,
                    render_debug,
                )
            render_debug.save_json(
                "prepare_render_assets_timing.json",
                prepare_trace.payload(),
            )

        return render_assets

    def frame_attribute_data(self, second: int, frame_number: int):
        attribute_data = {}
        valid_attributes = self.activity.valid_attributes
        frame_index = second * self.fps + frame_number
        for attribute in valid_attributes:
            if attribute == constant.ATTR_TIME:
                frame_timestamps = getattr(self.activity, "frame_timestamps", [])
                raw_times = getattr(self.activity, attribute, [])
                if frame_timestamps:
                    attribute_data[attribute] = frame_timestamps[
                        min(frame_index, len(frame_timestamps) - 1)
                    ]
                elif raw_times:
                    attribute_data[attribute] = raw_times[
                        min(second, len(raw_times) - 1)
                    ]
                else:
                    attribute_data[attribute] = None
            elif attribute in constant.NO_INTERPOLATE_ATTRIBUTES:
                attribute_data[attribute] = getattr(self.activity, attribute)[second]
            else:
                attribute_data[attribute] = getattr(self.activity, attribute)[
                    min(frame_index, len(getattr(self.activity, attribute)) - 1)
                ]
        return attribute_data

    def build_frame(self, total_frames, second, frame_number):
        frame_digits = max(1, len(str(max(total_frames - 1, 0))))
        from frame import Frame

        frame = Frame(
            f"{str(second * self.fps + frame_number).zfill(frame_digits)}.png",
            self.template["scene"]["width"],
            self.template["scene"]["height"],
            second,
            frame_number,
        )
        valid_attributes = self.activity.valid_attributes
        frame.valid_attributes = valid_attributes
        # frame.labels = self.labels
        frame_data = self.frame_attribute_data(second, frame_number)
        for attribute in frame.valid_attributes:
            setattr(frame, attribute, frame_data[attribute])
        return frame

    def iter_frames(self, seconds):
        total_frames = self.total_frame_count(seconds)
        for frame_index in range(total_frames):
            second, frame_number = self.frame_time_components(frame_index)
            yield self.build_frame(total_frames, second, frame_number)

    def build_frames(self, seconds):
        for frame in self.iter_frames(seconds):
            self.frames.append(frame)

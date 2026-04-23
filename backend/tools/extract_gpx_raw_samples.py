import argparse
import json
import xml.etree.ElementTree as ET
from pathlib import Path


def safe_number(value):
    if value is None:
        return None
    text = str(value).strip()
    if not text:
        return None
    try:
        return float(text)
    except ValueError:
        return None


def normalize_extension_key(value):
    return "".join(ch for ch in str(value or "").strip().lower() if ch.isalnum())


def local_name(tag):
    if "}" in tag:
        return tag.rsplit("}", 1)[1]
    return tag


def collect_leaf_extension_values(element, target):
    children = list(element)
    if not children:
        key = normalize_extension_key(local_name(element.tag))
        value = (element.text or "").strip()
        if key and value:
            target[key] = value
        return

    for child in children:
        collect_leaf_extension_values(child, target)


def read_track_point_metric(extension_values, aliases):
    for alias in aliases:
        normalized_alias = normalize_extension_key(alias)
        if normalized_alias not in extension_values:
            continue
        numeric_value = safe_number(extension_values[normalized_alias])
        if numeric_value is not None:
            return numeric_value
    return None


def first_text(parent, name):
    if parent is None:
        return None
    for element in parent.iter():
        if local_name(element.tag) == name:
            text = (element.text or "").strip()
            if text:
                return text
    return None


def parse_gpx(gpx_path: Path):
    tree = ET.parse(gpx_path)
    root = tree.getroot()

    metadata_node = next((node for node in root.iter() if local_name(node.tag) == "metadata"), None)
    track_node = next((node for node in root.iter() if local_name(node.tag) == "trk"), None)
    metadata_name = (
        first_text(metadata_node, "name")
        or first_text(track_node, "name")
    )

    raw_samples = []
    for track_point in (node for node in root.iter() if local_name(node.tag) == "trkpt"):
        extension_values = {}
        extensions_node = next(
            (child for child in track_point if local_name(child.tag) == "extensions"),
            None,
        )
        if extensions_node is not None:
            for child in extensions_node:
                collect_leaf_extension_values(child, extension_values)

        elevation = safe_number(first_text(track_point, "ele"))
        timestamp = first_text(track_point, "time")

        raw_samples.append(
            {
                "airPressure": read_track_point_metric(
                    extension_values, ["air_pressure", "absolute_pressure", "pressure"]
                ),
                "altitude": elevation,
                "cadence": read_track_point_metric(extension_values, ["cad", "cadence"]),
                "distance": read_track_point_metric(
                    extension_values, ["distance", "distance_m", "distancemeters"]
                ),
                "elevation": elevation,
                "gForce": read_track_point_metric(extension_values, ["g_force", "gforce"]),
                "gradient": read_track_point_metric(
                    extension_values, ["gradient", "grade", "slope"]
                ),
                "groundContactTime": read_track_point_metric(
                    extension_values,
                    ["ground_contact_time", "groundcontacttime", "stance_time"],
                ),
                "heading": read_track_point_metric(
                    extension_values, ["heading", "course", "bearing", "gps_heading"]
                ),
                "heartrate": read_track_point_metric(
                    extension_values, ["hr", "heartrate", "heart_rate"]
                ),
                "latitude": safe_number(track_point.get("lat")),
                "leftRightBalance": read_track_point_metric(
                    extension_values, ["left_right_balance", "leftrightbalance", "balance"]
                ),
                "longitude": safe_number(track_point.get("lon")),
                "pace": read_track_point_metric(extension_values, ["pace"]),
                "power": read_track_point_metric(
                    extension_values, ["power", "powerinwatts", "watts"]
                ),
                "speed": read_track_point_metric(extension_values, ["speed", "enhanced_speed"]),
                "strideLength": read_track_point_metric(
                    extension_values, ["stride_length", "stridelength", "step_length"]
                ),
                "strokeRate": read_track_point_metric(
                    extension_values, ["stroke_rate", "strokerate"]
                ),
                "temperature": read_track_point_metric(
                    extension_values, ["atemp", "temperature", "temp"]
                ),
                "timestamp": timestamp,
                "torque": read_track_point_metric(extension_values, ["torque"]),
                "verticalOscillation": read_track_point_metric(
                    extension_values, ["vertical_oscillation", "verticaloscillation"]
                ),
                "verticalSpeed": read_track_point_metric(
                    extension_values, ["vertical_speed", "verticalspeed", "vam"]
                ),
            }
        )

    return {
        "fileName": gpx_path.name,
        "fileFormat": "gpx",
        "metadata": {
            "activity_name": metadata_name,
            "creator": root.attrib.get("creator"),
        },
        "rawSamples": raw_samples,
        "options": {"useLegacyGpxDerivations": True},
    }


def parse_args():
    parser = argparse.ArgumentParser(
        description="Extract frontend-style rawSamples from a GPX file."
    )
    parser.add_argument("--gpx", required=True, help="GPX file path.")
    parser.add_argument("--out", required=True, help="Output JSON path.")
    return parser.parse_args()


def main():
    args = parse_args()
    payload = parse_gpx(Path(args.gpx))
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()

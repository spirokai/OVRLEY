import argparse
import os
import shutil
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
BACKEND_DIR = REPO_ROOT / "backend"

if str(BACKEND_DIR) not in sys.path:
    sys.path.insert(0, str(BACKEND_DIR))

from designer import demo_frame  # noqa: E402


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Render a single Python baseline preview frame and copy it to a stable output path."
    )
    parser.add_argument("--gpx", required=True, help="Path to the GPX file.")
    parser.add_argument("--template", required=True, help="Path to the template JSON file.")
    parser.add_argument("--second", required=True, type=int, help="Absolute activity second to render.")
    parser.add_argument("--out", required=True, help="Output PNG path.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    gpx_path = Path(args.gpx).resolve()
    template_path = Path(args.template).resolve()
    out_path = Path(args.out).resolve()

    os.chdir(BACKEND_DIR)

    scene = demo_frame(str(gpx_path), str(template_path), args.second, True)
    if isinstance(scene, dict) and scene.get("error"):
        raise SystemExit(scene["error"])
    if not getattr(scene, "frames", None):
        raise SystemExit("Python preview render did not produce a frame")

    source_path = Path(scene.frames[0].full_path()).resolve()
    if not source_path.is_file():
        raise SystemExit(f"Rendered frame not found: {source_path}")

    out_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source_path, out_path)
    print(out_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

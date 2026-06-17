import subprocess
import sys
from pathlib import Path
from os import environ
from shutil import which


def resolve_emcc() -> str:
    for candidate in ("emcc.bat", "emcc.exe", "emcc"):
        resolved = which(candidate)
        if resolved:
            return resolved

    emsdk = environ.get("EMSDK")
    if emsdk:
        for candidate in ("emcc.bat", "emcc.exe"):
            resolved = Path(emsdk) / "upstream" / "emscripten" / candidate
            if resolved.exists():
                return str(resolved)

    local_candidate = Path("H:/tools/emsdk/upstream/emscripten/emcc.bat")
    if local_candidate.exists():
        return str(local_candidate)

    return "emcc.bat"


def main() -> int:
    args = [arg for arg in sys.argv[1:] if arg != "-fwasm-exceptions"]
    return subprocess.run([resolve_emcc(), *args]).returncode


if __name__ == "__main__":
    raise SystemExit(main())

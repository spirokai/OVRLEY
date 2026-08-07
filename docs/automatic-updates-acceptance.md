## Two-Version Acceptance

Run this checklist on real Windows installed, Windows portable, Apple Silicon macOS, and Linux AppImage systems:

1. Install or run version `N` from its intended user-facing package.
2. Publish signed version `N+1` through the orchestrator and confirm the public `latest.json` endpoint.
3. Launch version `N` and confirm the prompt shows `N+1`.
4. Choose `Later`; confirm the editor remains usable and no second prompt appears during that launch.
5. Relaunch version `N`, choose `Update now`, and confirm download progress, signature verification, installation, and relaunch.
6. Confirm the resulting native application version is `N+1`.
7. Repeat with the machine offline; the failed check must be invisible to the user.
8. Exercise an invalid or unavailable updater payload; confirm `Update failed` is dismissible and version `N` remains usable.

For Windows portable ZIPs, verify `.ovrley-portable` is beside `OVRLEY.exe`, no updater request or prompt occurs, and the normal resources and FFmpeg operation still work.

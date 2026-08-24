# Configurable Render Output Specification

Status: ready-for-agent
Last updated: 2026-08-24

## Problem Statement

OVRLEY currently chooses every rendered video's destination internally and writes it to the platform Documents/OVRLEY directory. Users cannot choose another folder or filename. The render pipelines generate timestamped names internally, completion assumes every result is below the default output directory, and the Overlays action always opens that fixed directory.

Users need control over the complete output path without losing the existing defaults. They expect a native Save dialog with a prefilled filename, an editable absolute path, and the last accepted output directory to survive application restarts. OVRLEY must reject unavailable or unwritable destinations before expensive activity processing or render commencement. Existing files must never be overwritten without explicit confirmation.

## Solution

The render dialog will own one draft absolute output path and expose a native Save action. When the dialog opens, Rust will suggest a fresh timestamped filename in the last accepted output directory. If no remembered directory is available, the existing platform-resolved Documents/OVRLEY directory remains the default.

Transparent exports will use `overlay_<timestamp>.mov`, and composite exports will use `video_<timestamp>.mp4`. The selected render mode owns the extension. User-supplied extensions are replaced automatically, and switching modes preserves the filename stem while changing the extension.

A validated `RenderOutputTarget` will represent the request-specific destination in Rust. It will pass explicitly from render command ingress through the transparent or composite pipeline instead of entering template config or replacing stable application paths. Rust will probe the exact target before parsing or processing activity data. The single machine-readable `already_exists` rejection opens a dedicated overwrite-confirmation modal above the still-open render dialog; every other probe failure returns the operating system's error.

After the backend accepts a render, the frontend stores the accepted full path alongside the active render ID, derives its parent directory through the platform path API, and persists only that directory. Progress continues to expose a basename for presentation. Automatic opening uses the accepted absolute path, and Overlays opens the remembered directory.

## User Stories

1. As an OVRLEY user, I want to see the complete output path before rendering, so that I know exactly where the video will be written.
2. As an OVRLEY user, I want the output path to be absolute, so that its filesystem meaning is unambiguous.
3. As an OVRLEY user, I want to edit the complete path directly, so that I can change its directory or filename.
4. As an OVRLEY user, I want a Browse action, so that I can use my operating system's filesystem interface.
5. As an OVRLEY user, I want Browse to use a native Save dialog, so that folder and filename are chosen together.
6. As an OVRLEY user, I want the Save dialog prefilled with the current complete path, so that it preserves edits already made in the render dialog.
7. As an OVRLEY user, I want my first render to default to Documents/OVRLEY or its platform equivalent, so that existing convenience remains.
8. As an OVRLEY user, I want the default resolved through the operating system, so that redirected or localized Documents locations work.
9. As an OVRLEY user, I want app-owned Documents/OVRLEY created automatically, so that a first render needs no setup.
10. As an OVRLEY user, I want transparent renders suggested as `overlay_<timestamp>.mov`, so that existing naming remains recognizable.
11. As an OVRLEY user, I want composite renders suggested as `video_<timestamp>.mp4`, so that the name is concise.
12. As an OVRLEY user, I want a fresh suggestion whenever the render dialog opens, so that successive jobs do not default to one target.
13. As an OVRLEY user, I want that suggestion stable while the dialog is open, so that settings changes do not unexpectedly rename it.
14. As an OVRLEY user, I want transparent output to use `.mov`, so that the container matches its supported codecs.
15. As an OVRLEY user, I want composite output to use `.mp4`, so that the container matches H.264 and HEVC.
16. As an OVRLEY user, I want mode changes to preserve my filename stem, so that they do not discard naming work.
17. As an OVRLEY user, I want mode changes to switch `.mov` and `.mp4` automatically, so that the target remains valid.
18. As an OVRLEY user, I want a missing extension appended automatically, so that I need not type a container suffix.
19. As an OVRLEY user, I want an arbitrary final extension replaced automatically, so that render mode remains authoritative.
20. As an OVRLEY user, I want collision checks applied to the normalized path, so that confirmation covers the file actually written.
21. As an OVRLEY user, I want cancelling the Save dialog to leave the draft path unchanged, so that browsing has no side effects.
22. As an OVRLEY user, I want closing the render dialog to leave the remembered directory unchanged, so that exploration does not alter defaults.
23. As an OVRLEY user, I want typed and browsed destinations remembered equally, so that persistence does not depend on selection method.
24. As an OVRLEY user, I want a directory remembered only after backend acceptance, so that invalid or abandoned choices never become defaults.
25. As an OVRLEY user, I want an accepted directory remembered even if encoding later fails or is cancelled, so that it reflects my accepted choice.
26. As a returning user, I want the last accepted directory restored across sessions, so that repeated exports require less navigation.
27. As an OVRLEY user, I want only the directory persisted, so that each render still gets a fresh filename.
28. As an OVRLEY user, I want missing remembered state to fall back to Documents/OVRLEY, so that optional absence never blocks rendering.
29. As an OVRLEY user, I want a preference-store read failure to fall back to Documents/OVRLEY, so that temporary storage problems do not block rendering.
30. As an OVRLEY user, I want malformed present preference data reported, so that corrupted required state is not silently repaired.
31. As an OVRLEY user, I want preference-write failure not to affect an accepted render, so that ancillary persistence cannot invalidate rendering.
32. As an OVRLEY user, I want a non-blocking warning when a directory cannot be remembered, so that later fallback behavior is understandable.
33. As an OVRLEY user, I want relative output paths rejected, so that the backend never guesses their meaning.
34. As an OVRLEY user, I want missing parent directories rejected, so that typos do not create unintended directory trees.
35. As an OVRLEY user, I want arbitrary directories never created automatically, so that folder creation remains explicit.
36. As an OVRLEY user, I want non-directory path components rejected, so that invalid destinations fail immediately.
37. As an OVRLEY user, I want unwritable targets rejected, so that I do not wait through render preparation before a permissions error.
38. As an OVRLEY user, I want the exact requested path tested, so that platform filename and location restrictions are detected.
39. As an OVRLEY user, I want output validation before config parsing, activity processing, video probing, controller registration, or thread creation, so that failure is fast.
40. As an OVRLEY user, I want validation failure to leave the render dialog open, so that I can correct the target without recreating settings.
41. As an OVRLEY user, I want the rejected path preserved, so that I can see and edit it.
42. As an OVRLEY user, I want an existing target identified without parsing an error message, so that overwrite confirmation is reliable while other filesystem failures remain faithful to the operating system.
43. As an OVRLEY user, I want an existing target detected before rendering, so that it is never overwritten accidentally.
44. As an OVRLEY user, I want an existing target to open a second modal above the render dialog, so that overwrite consent is explicit.
45. As an OVRLEY user, I want cancelling overwrite confirmation to return to the unchanged render dialog, so that I can choose another name.
46. As an OVRLEY user, I want confirming overwrite to retry the same normalized target, so that consent cannot apply to another path.
47. As an OVRLEY user, I want any path or mode change to clear overwrite authorization, so that stale consent cannot overwrite a different file.
48. As an OVRLEY user, I accept direct confirmed overwrite, so that the implementation remains simple.
49. As an OVRLEY user, I understand that failed or cancelled confirmed overwrite may destroy the previous file, so that the behavior is explicit.
50. As an OVRLEY user, I want a successful new-file probe removed immediately, so that validation leaves no empty output.
51. As an OVRLEY user, I want existing partial-output cleanup preserved, so that failed new renders do not look complete.
52. As an OVRLEY user, I want post-validation filesystem changes reported as normal render failures, so that removable and network storage remain understandable.
53. As an OVRLEY user, I want progress to display only the basename, so that the interface remains uncluttered.
54. As an OVRLEY user, I want the completed video opened from its accepted absolute path, so that custom-directory results open correctly.
55. As an OVRLEY user, I want opening to fail clearly if the result was moved or deleted, so that stale filesystem state is visible.
56. As an OVRLEY user, I want Overlays to open my last accepted directory, so that I can find renders quickly.
57. As a first-time user, I want Overlays to open Documents/OVRLEY, so that it always has a meaningful target.
58. As an OVRLEY user, I want an unavailable remembered directory reported rather than silently redirected, so that I am not surprised.
59. As an OVRLEY user, I want preview-frame destinations unchanged, so that production output selection does not redirect diagnostic output.
60. As an OVRLEY user, I want template, runtime, debug, and temporary paths unaffected, so that one request cannot mutate application-wide paths.
61. As a developer, I want both pipelines to consume one validated output-target abstraction, so that they enforce one contract.
62. As a developer, I want render config and destination to remain separate, so that filesystem state never becomes template data.
63. As a developer, I want one canonical full target rather than directory and filename aliases, so that consumers cannot diverge.
64. As a developer, I want acceptance to return only the canonical output path, so that directory and basename remain derived.
65. As a developer, I want progress to retain its presentation basename without carrying a full path in every event, so that payloads stay focused.

## Implementation Decisions

- The render settings draft contains one complete absolute output path. Directory and basename are not parallel draft fields.
- The render dialog component remains presentational. Suggestion loading, native Save invocation, normalization, overwrite state, and submission coordination belong in hooks and utilities.
- The existing Tauri dialog plugin provides the native Save dialog; no custom native picker is required.
- The Save dialog receives the current full path as its default and a mode-appropriate `.mov` or `.mp4` filter.
- Rust owns generated naming. A command returns a suggested absolute target using the optional remembered directory or platform Documents/OVRLEY.
- Suggested transparent names use `overlay_<nanosecond timestamp>.mov`. Suggested composite names use `video_<nanosecond timestamp>.mp4`; `video_composited_` is retired for new output.
- One suggestion is generated per render-dialog session. Settings changes do not regenerate its timestamp or stem.
- The frontend normalizes user-edited and picker-returned paths once at UI ingress: remove the final supplied extension and append the mode-owned extension. Mode changes preserve the stem.
- Rust validates that the submitted extension matches the pipeline. This enforces the IPC contract rather than providing a second normalization path.
- A core `RenderOutputTarget` is the canonical validated request-specific destination. It is absent from template config and does not mutate `AppPaths`.
- Render ingress accepts the raw absolute output path and explicit overwrite authorization with existing render inputs. It constructs `RenderOutputTarget` before config or activity processing.
- `RenderOutputTarget` passes explicitly through transparent and composite orchestration. Pipelines stop deriving production targets from `downloads_dir`.
- Stable application paths continue to own resources, templates, runtime scratch data, debug output, preview output, and the platform default. Custom production output changes none of them.
- Probing stays simple and tests the exact target. Rust attempts exclusive creation. Success is closed and removed. `AlreadyExists` enters overwrite handling. With authorization, Rust opens the existing regular file for writing without truncation; FFmpeg performs truncation.
- A cleanup guard prevents ordinary early returns from leaving a new empty probe. No directory walking, recursive creation, permission-bit prediction, or redundant preflight is added.
- Only Documents/OVRLEY is app-created. Missing arbitrary parents are rejected.
- Output probing is authoritative at acceptance, while later filesystem changes remain possible and use the existing render-error lifecycle.
- Exclusive creation returning `ErrorKind::AlreadyExists` crosses Tauri as `{ code: 'already_exists', message }`. This is the only OVRLEY-owned output error code because it is the only error that changes frontend behavior.
- Every other exact-path probe failure returns the operating system's original I/O error. OVRLEY does not classify missing parents, non-directory components, permissions, invalid names, extensions, or miscellaneous I/O failures into an application taxonomy.
- Only `already_exists` opens overwrite confirmation. All other errors leave the confirmation phase and complete draft intact.
- The `backend_render` Tauri command returns `Result<String, BackendRenderError>`: the success arm is the existing JSON string, and the error arm is a structured enum serializing to either `{ code: 'already_exists', message }` or `{ code: 'render_error', message }`. The shared `call_and_serialize` helper is not changed.
- Overwrite confirmation is a second modal above the render dialog. Confirmation retries the unchanged normalized target with authorization. Editing, browsing, or mode-driven path change clears authorization.
- Confirmed overwrite uses FFmpeg's direct output behavior. Staging, backup, rollback, and cross-platform atomic replacement are not introduced. The prior file can be lost after failure or cancellation.
- A render is accepted only after synchronous command validation succeeds and the controller registers it. Output rejection does not transition permanently to progress.
- The acceptance response exposes canonical `outputPath`, not duplicate output-directory or basename fields.
- The frontend stores accepted `outputPath` alongside `activeRenderId`, derives its directory using Tauri's platform path API, and persists only that directory under one canonical last-render-output key.
- Preference updates occur after acceptance regardless of encoding outcome. Rejection and all dialog/modal cancellations do not update them.
- Preference-write failure is ancillary: it warns without cancelling or failing an accepted render, and leaves the previous preference unchanged.
- Missing or unreadable optional preference state falls back to Documents/OVRLEY. A malformed present type or non-absolute value fails loudly.
- Progress continues reporting `filename` as a basename. Full paths are not added to progress events.
- Completion opening uses the accepted output path stored with the active render. The open-video command checks only that the current target is a file, not render-target extension, writability, or overwrite rules.
- Overlays reads the remembered directory and passes it to the native open-directory command. Absence uses the default; an inaccessible present directory errors without fallback.
- Existing completion, cancellation, progress, codec, range, and render-mode behavior remains unchanged except where this specification says otherwise.

## Testing Decisions

- Tests assert external behavior at the existing frontend render-workflow and Rust command/pipeline seams. No new broad harness is introduced.
- Add one parameterized frontend test for `.mov`/`.mp4` normalization, supplied-extension replacement, and stem preservation.
- Add one frontend workflow test covering ordinary OS-error rejection, `already_exists`, overwrite confirmation, and authorized retry of the same path.
- Add one frontend workflow test proving only acceptance enters progress, persists the directory, and later opens the accepted absolute path.
- Add one Rust output-target test covering new-target probe cleanup, existing-target rejection, and authorized existing-target validation without truncation.
- Add one Rust command test proving output rejection precedes malformed activity processing and leaves `RenderController` inactive.
- Adapt one existing transparent and one existing composite pipeline case to assert explicit custom targets; do not create parallel pipeline matrices.
- Do not add dedicated tests for every OS error, preference permutation, dialog button, API wrapper, opener command, timestamp value, or platform-specific permission behavior.
- Run the existing frontend and standalone core suites after the focused tests. A production build remains excluded unless separately authorized.

## Out of Scope

- Styling, wording, icon choice, and other inferable presentation details.
- Containers beyond `.mov` for transparent and `.mp4` for composite.
- Exposing low-level template container overrides as output formats.
- Persisting a complete last filename or old timestamped suggestion.
- Per-mode remembered directories; both modes share one last accepted directory.
- Automatically creating arbitrary missing directory trees.
- Open-time or continuous filesystem preflight.
- Silently redirecting an invalid remembered directory.
- Transactional staging, backup, rollback, or atomic replacement.
- Preserving an overwritten file after FFmpeg starts and later fails or is cancelled.
- Changing preview, debug, template, runtime, or temporary destinations.
- Adding full paths to progress events or the progress display.
- Reworking unrelated render settings, codecs, acceleration, ranges, activity parsing, or synchronization.
- Concurrent render jobs; the existing single-active-controller contract remains.

## Further Notes

- The Save dialog may use its platform fallback when a remembered location is stale. OVRLEY does not preflight availability merely to choose the dialog's opening location.
- Because the path is directly editable, native overwrite confirmation cannot be the sole authorization. Structured backend rejection and the second modal provide one contract for browsed and typed paths.
- Extension replacement is intentional normalization: users own the stem; the pipeline owns the container.
- Keep the output probe to the simplest exact filesystem operation. Do not expand it into recursive inspection or permission prediction.
- Production render entry points adopt explicit `RenderOutputTarget`. Test and benchmark helpers may construct targets from fixture directories as appropriate.
- No ADR governs this area. The design follows strict ingress validation, canonical shapes, owner-fixed mismatches, and presentational components.

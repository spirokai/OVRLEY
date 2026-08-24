# Configurable Render Output Implementation Plan

Status: ready-for-agent
Spec: `output-spec.md`
Last updated: 2026-08-24

## Objective

Implement configurable absolute output paths for production video renders while preserving the current render modes, progress lifecycle, cancellation behavior, preview output, and platform Documents/OVRLEY default.

The implementation must establish one request-owned `RenderOutputTarget`, validate it before expensive render preparation, preserve the render dialog on rejection, require explicit overwrite authorization, remember only the last accepted directory, and route completion/open-folder actions to the correct custom path.

## Architectural Shape

The feature spans four ownership layers:

1. **Rust core output contract** â€” owns suggested names, output-kind/extension rules, exact-path probing, structured output errors, and validated `RenderOutputTarget`.
2. **Rust render orchestration and pipelines** â€” accept one validated target explicitly and never derive production output from `AppPaths.downloads_dir`.
3. **Tauri/API boundary** â€” exposes suggestion, render-target inputs, the single machine-readable collision rejection, exact-path opening, and remembered-directory opening without hiding operating-system errors.
4. **Frontend workflow and presentation** â€” owns the dialog draft, Save interaction, extension normalization, overwrite modal, transient accepted job state, and optional preference persistence.

The target does not enter template config. `AppPaths` remains stable runtime configuration. Directory and basename are derived from the one full path rather than stored as aliases.

## Phase 1 â€” Lock the Contracts with Focused Tests

Use only the existing Rust command/pipeline seam and frontend render-workflow seam. Do not add component, API-wrapper, preference-helper, opener-command, or platform-error test suites for this feature.

- Add one parameterized pure test for the filename rule: transparent becomes `.mov`, composite becomes `.mp4`, supplied extensions are replaced, and the stem is preserved.
- Add one frontend workflow test for rejection: an ordinary operating-system error preserves the confirmation draft, while `already_exists` opens overwrite confirmation and the confirmed retry sends `overwrite: true` for the same path.
- Add one frontend workflow test for acceptance: only an accepted response enters progress, persists the accepted directory, and later opens the accepted absolute output path.
- Add one Rust output-target test covering the three essential filesystem branches: new target probe/cleanup, existing target rejection, and authorized existing target acceptance without validation-time truncation.
- Add one Rust command test proving output rejection occurs before malformed activity processing and leaves `RenderController` inactive.
- Adapt one existing transparent and one existing composite pipeline integration test to assert that each writes to its explicit custom target. Do not create parallel pipeline suites.

All other behavior should be covered by the owning implementation, existing regression tests, compilation, and a short manual smoke test. Avoid tests for individual error-code mappings, dialog button wiring, preference read/write permutations, generated timestamp values, or OS-specific permission behavior.

## Phase 2 â€” Introduce the Rust Output Domain

Create a neutral core module for render-output contracts so command orchestration and both encode pipelines can depend on it without reverse-depending on one another.

### Canonical types

- Add `RenderOutputKind` with exactly `Transparent` and `Composite`.
- Deserialize its external values from the render request's canonical mode names.
- Give each kind ownership of:
  - Required extension (`mov` or `mp4`).
  - Generated filename prefix (`overlay` or `video`).
- Add `RenderOutputTarget` containing one private canonical absolute `PathBuf` and its `RenderOutputKind`.
- Expose read-only accessors needed by pipelines: full path and basename.
- Do not store a parallel directory or filename inside the type.

### Error contract

- Add one serializable machine-readable rejection: `{ code: 'already_exists', message }`.
- Emit it only when exclusive creation reports `ErrorKind::AlreadyExists` and overwrite authorization is absent.
- Return every other exact-path probe failure using the operating system's original I/O error text. Do not classify missing parents, non-directory components, permissions, invalid names, or other I/O failures into OVRLEY-owned codes.
- Add a render-command error wrapper only as needed to preserve `already_exists` across Tauri without parsing its message; ordinary core and operating-system failures remain ordinary errors.

### Suggested output

- Move production filename generation out of the pipelines into the output module.
- Reuse the existing nanosecond timestamp helper or move timestamp generation to a neutral owner if necessary.
- Implement suggestion from:
  - A validated optional remembered absolute directory; or
  - `AppPaths.downloads_dir` when the preference is absent/unavailable.
- Join the generated basename in Rust and return one absolute path.
- Do not check directory availability while producing a suggestion.

### Exact-target validation

- Validate in this order:
  1. Non-empty absolute path with a usable filename.
  2. Extension matches `RenderOutputKind` case-insensitively.
  3. Attempt exclusive creation of the exact target.
- When exclusive creation succeeds:
  - Close the empty probe.
  - Remove it immediately.
  - Return a validated target.
- When exclusive creation reports `AlreadyExists`:
  - Without authorization, return `already_exists` immediately.
  - With authorization, require an existing regular file and open it for writing without truncation.
- Return every other operating-system error directly rather than adding predictive inspection or an application error taxonomy.
- Use a small RAII cleanup guard for a newly created probe so early error returns do not leave an empty file.
- Do not recursively create directories, walk ancestors, inspect permission bits, stage outputs, back up existing files, or implement replacement transactions.

## Phase 3 â€” Make Render Output an Explicit Pipeline Dependency

Thread `RenderOutputTarget` through production render orchestration before adding frontend behavior.

### Command orchestration

- Extend the core render command signature with raw output path, output kind, and overwrite authorization.
- Construct and probe `RenderOutputTarget` before parsing config JSON or activity JSON.
- Parse and validate render config only after the output target passes.
- Assert that output kind and validated config render mode agree. Treat disagreement as malformed request input; do not silently switch modes.
- Clone/move the validated target into the background render closure together with the validated config and parsed activity.
- Return `started`, `render_id`, and canonical `outputPath` after controller registration.

### Transparent pipeline

- Add an explicit target parameter to the production transparent render entry point.
- Remove internal public filename/timestamp construction.
- Pass the target path directly to FFmpeg and `PartialOutputGuard`.
- Return only the target basename to `RenderController.finish_success`, preserving the presentation-oriented progress contract.
- Keep transparent debug-directory generation independent from the public filename.

### Composite pipeline

- Add the target to composite pipeline-plan derivation rather than reading `downloads_dir`.
- Remove `video_composited_<timestamp>.mp4` construction.
- Pass the plan's explicit target to FFmpeg, verification, cleanup, and completion.
- Return only the target basename to progress.
- Update composite diagnostics so debug-directory identity no longer strips or requires the old `video_composited_` prefix. Generate an internal debug ID independently; arbitrary user stems must be valid production targets.

### Non-production callers

- Update command tests, integration fixtures, render baseline helpers, and benchmark binaries to construct explicit targets in their own fixture/output directories.
- Keep preview-frame rendering on `AppPaths.downloads_dir` unchanged.
- Do not replace `downloads_dir` globally or mutate cloned `AppPaths` as an adapter.

## Phase 4 â€” Preserve Structured Errors Across Tauri IPC

The current Tauri wrapper converts every core error to `String`, and the frontend normalizer discards object metadata. Both owners must change for the overwrite flow.

### Tauri shell

- Extend `backend_render` arguments with `outputPath`, `outputKind`, and `overwrite`.
- Return the serialized accepted result as today.
- Give this command a serializable error type instead of routing it through the string-flattening `call_and_serialize` helper.
- Map runtime path/setup failures to a general render command error without pretending they are output-target classifications.
- Register a suggestion command accepting output kind and an optional remembered directory and returning one absolute path.
- Change the video-opening command to accept an absolute `outputPath` and verify only that it currently identifies a file before invoking the OS opener.
- Replace the fixed downloads-opening command with an output-directory command accepting an optional absolute directory:
  - `None` opens `AppPaths.downloads_dir`.
  - A present value must be absolute and currently be a directory.
  - A missing/inaccessible present directory returns an error and never falls back silently.
- Update command registration and command documentation.

### Frontend API

- Extend `renderVideo` to send output path, output kind, and overwrite authorization as separate IPC fields, not inside config JSON.
- Add an API helper for suggested output paths.
- Change `openVideo` to accept a complete output path.
- Change `openDownloads`/its canonical replacement to accept the selected directory or optional default.
- Preserve structured Tauri error objects in `normalizeBackendError`:
  - Standard errors remain `Error` instances.
  - A valid backend `{ code, message }` rejection becomes an error object retaining `code`.
  - Do not introduce message parsing or compatibility aliases.
- Add API boundary tests proving structured codes survive invocation normalization.

## Phase 5 â€” Add Frontend Path and Preference Utilities

Keep filesystem and normalization logic out of the presentational dialog.

### Preference contract

- Define one canonical key, such as `last-render-output-dir`.
- Add a focused loader that distinguishes:
  - Missing value: return optional absence.
  - Store read failure: log/report a non-blocking warning and return optional absence.
  - Present valid absolute string: return it.
  - Present malformed value: throw loudly.
- Validate absoluteness through Tauri's platform path API rather than regex or manual separator logic.
- After acceptance, derive the parent with Tauri's `dirname` and persist only that directory.
- A write failure logs/reports a non-blocking warning and does not alter the active render lifecycle.

### Path normalization

- Add a pure utility mapping render mode to required extension.
- Normalize the final filename component without parsing Windows paths using POSIX-only utilities.
- Prefer Tauri platform path functions for dirname/basename/join operations; the pure utility should operate only on a supplied filename component if that keeps it platform-neutral.
- Replace the final supplied extension and append the required one when missing.
- Preserve the path's directory and filename stem.
- Ensure normalization is applied when:
  - A user commits a manual path edit.
  - The native Save dialog returns a path.
  - Export mode changes.
  - Start Render creates its final submission snapshot.

### Native Save helper

- Add a Save-path helper alongside existing open-file helpers.
- Accept the current full default path and the single allowed extension.
- Return `null` on cancellation and never update preferences itself.
- Keep last-directory persistence out of generic dialog helpers because this feature persists only after render acceptance.

## Phase 6 â€” Extend Render Dialog State and Presentation

Keep `RenderVideoDialog` presentational and put stateful behavior in the existing render hooks.

### Dialog draft initialization

- Make dialog opening resolve its output suggestion before publishing a non-null confirmation draft.
- Determine initial output kind from the same export-mode decision already used to build codec defaults.
- Load the optional remembered directory, request a Rust suggestion, and add `outputPath` to the freshly built draft.
- If suggestion initialization fails because of malformed present preference or backend failure, keep the dialog closed and surface the error; never create a partial required draft.
- Prevent duplicate asynchronous opens while suggestion loading is in flight.

### Output path behavior

- Add handlers for manual commit, Browse, and mode changes.
- Manual commit and Browse normalize the extension before updating the draft.
- Mode changes update export mode/codec settings and normalized output path in one draft update so consumers never observe mismatched committed state.
- Clear output-path error and overwrite authorization whenever the normalized target changes.
- The Start action creates a canonical normalized snapshot synchronously before submission.

### Presentational controls

- Add the text input and attached Browse button to the confirmation content.
- Show a path-specific backend error without closing the dialog.
- Disable duplicate Start submissions while the acceptance IPC is pending.
- Add a dedicated presentational overwrite-confirmation dialog layered above the render dialog.
- The overwrite dialog receives only open state, target basename/path presentation data, confirm callback, and cancel callback; it owns no render logic.

## Phase 7 â€” Correct the Render Acceptance Lifecycle

Refactor ownership so the render is not treated as active before the backend accepts it.

### Make render preparation side-effect free

- Change the render utility to:
  - Build the render-effective config.
  - Validate frontend-owned render inputs.
  - Invoke the backend with config, activity, output kind/path, and overwrite flag.
  - Return the accepted response or throw the structured rejection.
- Remove `setRenderingVideo`, `setActiveRenderId`, and progress mutation arguments from the utility.
- Let the workflow own all UI/store lifecycle transitions.

### Canonical active-job state

- Replace the standalone active render ID with one transient active-render-job object containing `id` and accepted `outputPath`.
- Provide one store action to set/clear the pair so ID and path cannot drift.
- Update progress filtering to compare event render ID against the active job's ID.
- On accepted response, atomically establish active job/rendering/progress state and move the dialog to `progress`.
- Remove the unused standalone `videoFilename` state/setter if it remains unconsumed; progress already owns the display basename.

### Rejection and overwrite flow

- Keep dialog phase `confirm` while the initial render IPC is pending.
- On `already_exists`, store the exact canonical pending path and open overwrite confirmation without setting render progress/error state.
- On overwrite cancel, clear only the pending authorization/modal state.
- On overwrite confirm, verify the current draft still normalizes to the pending path, then resubmit with `overwrite: true`.
- On other output codes, retain the full settings draft and expose the message as the path error.
- On non-output preparation/backend errors, preserve existing global error behavior unless the error is specifically actionable inside the path control.

### Acceptance and completion

- After acceptance, persist `dirname(response.outputPath)` asynchronously; failure warns but does not unwind accepted state.
- Completion continues deriving/displaying basename from progress.
- On successful completion, read `activeRenderJob.outputPath`, clear active job/rendering state, and call `openVideo` with the exact path.
- Cancellation and error clear the complete active job object.
- Preserve the existing initial progress snapshot so a very short render that finishes before subscription setup is still observed.

## Phase 8 â€” Route the Overlays Action to the Remembered Directory

- Move remembered-output-directory loading into a shared feature utility used by both render-dialog opening and the app-shell Overlays handler.
- On Overlays invocation:
  - Load optional remembered directory with the agreed absence/read-failure behavior.
  - Call the output-directory command with that value.
  - Let `None` resolve to Documents/OVRLEY in Rust.
  - Surface a present-but-unavailable directory error through the existing shell error channel.
- Update keyboard shortcut behavior indirectly through the existing shared handler so button and shortcut cannot diverge.
- Rename misleading internal `downloads` handler/API names where they now mean the remembered render output directory; do not retain aliases.

## Phase 9 â€” Complete Regression Coverage

Keep the feature-specific automated coverage to the six tests identified in Phase 1:

1. One parameterized frontend filename-normalization test.
2. One frontend rejection/overwrite workflow test.
3. One frontend acceptance/persistence/completion-path workflow test.
4. One Rust `RenderOutputTarget` filesystem-branch test.
5. One Rust command-ordering rejection test.
6. One existing transparent plus one existing composite pipeline case adapted in place; treat these as a single cross-pipeline contract check, not a new matrix.

Update existing assertions that depend on `video_composited_` or old function signatures only where required to keep their original contract valid. Do not multiply cases for every error code, platform, preference failure, input method, or modal interaction.

## Phase 10 â€” Verification and Review Gates

Run verification in increasing cost order. Do not run a production build.

1. Run the three focused frontend feature tests.
2. Run the focused Rust target/command tests and the two adapted pipeline cases.
3. Run frontend lint and formatting checks; fix only files touched by the feature.
4. Run the full frontend Vitest suite from the frontend package.
5. Run the full standalone core Cargo test suite.
6. Review the final diff for:
   - No output-path fields added to template config.
   - No mutable/replaced `AppPaths.downloads_dir` adapter.
   - No duplicated directory/filename state.
   - No message parsing for overwrite behavior.
   - No fallback from malformed present preference data.
   - No arbitrary directory creation.
   - No open-time availability preflight.
   - No staged-output or replacement abstraction.
   - No remaining production assumptions about `video_composited_`.
7. Perform one short manual smoke pass on the available platform: render to a custom path, reject one invalid path, confirm one overwrite, reopen the dialog to verify the remembered directory, and invoke Overlays.

## Suggested Commit Boundaries

1. Add tested Rust `RenderOutputTarget`, suggestion, and `already_exists` rejection.
2. Thread explicit targets through transparent/composite pipelines and migrate Rust callers/tests.
3. Expose Tauri/API contracts while preserving structured rejection.
4. Add frontend path/preference utilities and native Save behavior.
5. Refactor render acceptance state and add overwrite interaction.
6. Route completion and Overlays to accepted/remembered paths.
7. Finish regression tests and documentation cleanup.

Each boundary should compile and keep its owned tests green before moving onward. Do not retain temporary compatibility overloads between boundaries; update owners and callers together.

## Known Tradeoffs to Preserve

- Confirmed overwrite is direct, not transactional. Once FFmpeg truncates the target, failure or cancellation may destroy the old file.
- Exact-target probing reduces early failures but cannot prevent filesystem changes after validation.
- Preference persistence is optional after acceptance and cannot fail an active render.
- The native Save dialog may fall back to its platform default when handed a stale location; OVRLEY performs no open-time filesystem preflight.
- Only `.mov` and `.mp4` are supported by this feature.

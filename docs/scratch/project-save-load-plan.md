# Project save/load implementation plan

## Goal

Implement the project lifecycle described in `.agents/scratch/projects/PRD.md` as a versioned, strict OVRLEY JSON contract that saves project-owned sources and settings without serializing general editor or runtime Zustand state.

Project operations will live in their own left-toolbar drawer. Add `ProjectsDrawerContent` under `app/src/features/toolbar/components/` and wire it into the shared drawer alongside Activity, Video, and Widgets. Do not put project open/save controls in the template selector, app header, activity drawer, or video drawer.

The implementation must preserve these invariants:

- A project references one canonical template and embeds a normalized fallback only for explicit missing-template recovery.
- Existing canonical templates always win over the fallback, without revision comparison or change warnings.
- Activity and video files remain external sources and are never embedded.
- The project stores only source locators for activity and video. It stores no parsed activity, activity summary, activity-derived series, video metadata, probe result, video telemetry, preview state, or other cached/derived representation of either source.
- Project validation and normalization happen once at the project ingress boundary. Malformed present data fails; it is not repaired by consumer fallbacks.
- Every project load reparses the activity file and fully reimports the video from their source files. Activity restoration runs the existing format-specific parser and canonical finalizer; video restoration runs the existing probe, preview registration, and telemetry extraction pipeline. No project load may hydrate either object from serialized metadata or parsed data.
- Loading is staged. Project validation, template resolution, source resolution, parsing, and probing complete before the current editor session is replaced.
- A successful load commits one complete target session, clears undo/redo history, and starts paused. A pre-commit failure or cancellation leaves the current session unchanged.
- The only editor/player position state owned by a project is the playhead second and visible timeline range. Canvas zoom, background mode, grid visibility, snap-to-grid, mute, interpolation preferences, widget selection, and other editor presentation state are not project data and are not serialized or restored.
- Template dirty state and project dirty state remain separate.
- The loaded codec is retained even when unavailable locally; availability is enforced only when opening export settings or starting a render.
- The project UI is presentational. Lifecycle orchestration belongs in a project hook, pure projection and transformation in project utilities, and durable state/actions in store slices.

## Contract decisions

### File identity

- Use `.oly` as the project-specific extension.
- Use the native dialog filter `OVRLEY Project (*.oly)`.
- Persist the last directory used by the Open Project picker under `LAST_PROJECT_OPEN_DIR_KEY = "last-project-open-dir"`, using the existing preferences-store/file-dialog mechanism. This follows the established `last-<domain>-<purpose>-dir` nomenclature used by `last-activity-import-dir`, `last-video-import-dir`, and `last-render-output-dir`; `open` matches the user-facing Open Project operation. Define the key once in the project feature and import the constant wherever it is read, written, or tested. The next Open Project dialog initializes from that directory across application sessions. Update the preference only after the user selects a project path; cancellation must leave the remembered directory unchanged. This preference is application UI state and is never written into a project file.
- `.oly` is a ZIP container from version 1, not a plain JSON file. The archive layout is:

  ```text
  Race.oly
  ├── project.json   # required
  └── preview.png    # optional, reserved for a future canvas screenshot
  ```

- `project.json` is the authoritative UTF-8 project contract. Stamp it with `format: "ovrley-project"`, `version: 1`, and an ISO-8601 `savedAt` value.
- `preview.png` is a non-authoritative presentation asset. Its presence or absence must never affect validation, loading, dirty status, or hydration. Screenshot capture/display remains out of scope for this implementation; reserving the entry avoids changing the `.oly` container format later.
- Write `project.json` with Deflate compression. PNG is already compressed, so a future `preview.png` entry should use ZIP's Stored method rather than recompressing it.
- Read entries directly from the archive without extracting them to the filesystem. Version 1 permits exactly one root-level `project.json` and at most one root-level `preview.png`; reject missing/duplicate required entries and other archive paths. Cap `project.json` at 4 MiB, `preview.png` at 32 MiB, and the complete `.oly` file at 40 MiB before allocating entry buffers.
- Reject unsupported formats, unsupported versions, missing required keys, invalid enums, invalid numeric ranges, invalid booleans, malformed locators, malformed template fallback data, and unknown keys where the contract is intended to be closed.

### Version 1 payload

Use one canonical camelCase disk shape:

```json
{
  "format": "ovrley-project",
  "version": 1,
  "savedAt": "2026-08-27T12:00:00.000Z",
  "template": {
    "source": {
      "kind": "file",
      "path": {
        "kind": "project-relative",
        "value": "templates/track-day.json"
      }
    },
    "fallback": {
      "config": {},
      "settings": {
        "globalDefaults": {}
      }
    }
  },
  "sources": {
    "activity": {
      "path": {
        "kind": "project-relative",
        "value": "media/session.fit"
      }
    },
    "video": null
  },
  "sync": {
    "videoOffsetSeconds": 0,
    "videoTimezoneMode": null
  },
  "render": {
    "fps": 30,
    "widgetUpdateRate": 1,
    "exportMode": "transparent",
    "codec": "prores_ks",
    "bitrateMbps": null,
    "range": {
      "type": "all",
      "from": 0,
      "to": 0
    }
  },
  "timeline": {
    "playheadSecond": 0,
    "viewStart": 0,
    "viewEnd": 73
  }
}
```

Contract details:

- `template.source` is a strict discriminated union:
  - `{ kind: "file", path: PathLocator }` for every user-owned template backed by a filesystem file. This includes templates saved under `Documents/OVRLEY/templates`, templates opened from another directory, and templates saved to an arbitrary user-selected path.
  - `{ kind: "bundled", templateId }` only for read-only templates shipped inside the application bundle, where no stable user filesystem path exists.
- A project must persist the actual locator of its user template. Membership in the backend template list is not a substitute for the path and must not turn a user template into an identifier-only reference.
- Template locators use the same project-relative-or-absolute `PathLocator` contract as activity and video sources. This allows a project and user template moved together to keep working while still supporting templates stored anywhere on disk.
- Replace the current `loadedTemplateFilename` plus string `loadedTemplateSource` pairing with one canonical template source descriptor owned by the template slice. Update all template consumers to use that descriptor; do not retain aliases for the old shape.
- `template.fallback` is exactly the durable normalized template state produced by `createDurableTemplateState`. It is required in every saved project and is never selected automatically.
- `PathLocator` is `{ kind: "project-relative", value }` or `{ kind: "absolute", value }`. In both variants, `value` is the complete path including the filename; never store the directory and filename separately. A project-relative value resolves from the directory containing the `.oly` file. For example, if `C:\Events\Race.oly` references `templates/track-day.json`, it resolves to `C:\Events\templates\track-day.json`. Its value is portability: moving or copying the complete `C:\Events` folder preserves the reference even when its absolute location or drive changes. Use `project-relative` only when the referenced file is inside the project directory tree. For files elsewhere, store an absolute locator such as `{ kind: "absolute", value: "D:\\OVRLEY Templates\\track-day.json" }`. Do not create `..`-based relative locators that reach outside the project tree.
- `sources.activity` and `sources.video` are independently nullable. When present, each is a strict source descriptor containing only its locator. The activity/video format, filename display value, timestamps, duration, frame rate, resolution, codec, bitrate, camera data, summaries, and telemetry are derived again from the resolved source during load; do not duplicate any of them in the project.
- Background images are editor-preview state, not project media. Do not include an image locator or restore an imported background image during project loading.
- `sync.videoTimezoneMode` accepts only `"local"`, `"utc"`, or `null`.
- `render.exportMode` accepts only `"transparent"` or `"composite"`. Composite mode requires a video source. `bitrateMbps` is a positive finite number or `null`; `null` is canonical when the selected codec/mode does not use bitrate.
- `render.range` retains the existing canonical `{ type, from, to }` shape. `type` is `"all"` or `"custom"`; custom ranges require finite `from < to`. Do not silently replace malformed ranges with defaults.
- `timeline.viewStart < timeline.viewEnd` is required at ingress. After media is reparsed, clamp this already-valid range to the reconstructed timeline bounds using the existing viewport utility.

The contract must exclude all parsed or derived activity/video data, including parsed activity, raw or normalized samples, metric series, activity summary, video telemetry, probed video metadata, duration, FPS, resolution, source codec/bitrate, camera information, creation timestamps, preview URLs/import IDs/warnings, and caches. It must also exclude canvas zoom, background mode, grid visibility, snap-to-grid, mute, preview interpolation, widget selection, template lists, platform and codec availability, render/import status, errors, dialog drafts, playback/scrub state other than the saved paused playhead, hover/drag state, editor instances, DOM measurements, UI scale, debug mode, and undo/redo history.

### Archive dependency

Add one dependency to `src-tauri/Cargo.toml`:

```toml
zip = { version = "=2.4.2", default-features = false, features = ["deflate"] }
```

- Keep archive handling in the Tauri shell; do not add `zip` to `ovrley_core`.
- Pin `2.4.2` because it supports Rust 1.73 and therefore the Tauri crate's declared Rust 1.85 toolchain. The current `zip` 8.6 release requires Rust 1.88 and cannot be used without a separate toolchain upgrade.
- Disable the broad default feature set because `.oly` needs only Stored entries and Deflate. No encryption, Bzip2, Zstandard, LZMA, XZ, or time conversion support is needed.
- No pnpm dependency, external executable, or system ZIP library is required. The Rust crate owns archive reading/writing and its Deflate implementation is pulled through Cargo.

## Target architecture

### Project feature

Add `app/src/features/projects/` with these responsibilities:

- `utils/projectSnapshot.js`: create the explicit durable project payload, stamp/stringify it, and derive a small dirty-comparison projection containing only template reference, source locators, sync, committed render settings, playhead, and timeline range. The comparison projection excludes `savedAt` and the embedded template fallback so timestamp refreshes or canonical template edits do not make the project appear Modified.
- `utils/projectPaths.js`: create and resolve the canonical path locator shape at the native boundary. Keep platform path handling in one place.
- `utils/projectHydration.js`: pure construction of the final store payload, including timeline/playhead clamping after reconstructed media durations are known.
- `hooks/useProjectLifecycle.js`: own Open, Save, Save As, recovery dialogs, staged dependency reconstruction, atomic commit, project identity, busy/error outcomes, interaction with template save status, and use of the persisted Open Project directory preference.
- `components/MissingTemplateDialog.jsx`: present Locate, Use Embedded Fallback, and Cancel.
- `components/MissingSourceDialog.jsx`: present Locate and Cancel for a moved activity or video.
- `index.js`: expose only the feature's public API.

The lifecycle hook should accept the existing template/activity/video boundaries as dependencies where practical. This makes orchestration tests exercise observable outcomes without mocking private setter order.

### State ownership

Do not add a broad `createProjectSlice`. Project persistence is an orchestration concern over state already owned by the template, media, render, and timeline domains; a project slice containing copies of those values would create duplicate ownership.

- `useProjectLifecycle` owns `loadedProjectPath` and `lastSavedProjectState` as hook-local lifecycle bookkeeping. `loadedProjectPath` selects Save versus Save As. `lastSavedProjectState` is the small dirty-comparison projection used only to derive `Saved` versus `Modified`; it is not written into the project file and contains neither the template fallback nor runtime activity/video data.
- If project identity later needs to be consumed outside the single app-shell/project-drawer composition, lift only that identity bookkeeping into a narrowly scoped store slice. Do not move render, timeline, template, or media domain values into it.
- Keep editor canvas zoom, background mode, grid visibility, and snap-to-grid in `useEditorShellState`. Project work must not move or persist them. Keep UI scale, keyboard workspace/dialog state, development debug mode, mute, preview interpolation, and widget selection at their existing editor/player owners as well.
- Add the visible timeline viewport `{ viewStart, viewEnd }` beside the existing playhead/timeline state in `createEditorSlice`, because it must be readable by save and atomically set during load. `useTimelineViewport` continues to own measurement, ticks, gestures, fitting, and follow behavior through strict viewport actions.
- Add a focused `createRenderSettingsSlice.js` for the canonical committed FPS, widget update rate, export mode, codec, optional bitrate, and export range aggregate. Remove the split render-setting ownership from `createTemplateSlice` after updating consumers; do not add compatibility aliases.
- Add the original native activity path/source descriptor to media state at activity ingress. Replace filename-only ownership with a canonical activity source object from which the filename is derived for presentation.
- Keep reconstructed video metadata and preview state in `createVideoImportSlice`; only the video source path and committed sync values participate in project projection.
- Implement atomic hydration as a project utility that performs one `useStore.setState` Immer transaction inside `replaceEditorDocument`. It writes each staged value to its existing owning domain and is not a persistent store action.

Decide project dirty state by comparing a fresh dirty-comparison projection with the hook-local `lastSavedProjectState`. Exclude `savedAt` and `template.fallback`: the fallback is refreshed as a save side effect, while canonical template modifications belong to template status rather than project status.

### Native boundary

Add narrowly scoped project commands in a dedicated `src-tauri/src/project_file.rs` module and register them in `src-tauri/src/lib.rs`:

- `read_project_file(path)`: open the ZIP container, read the required root `project.json` entry under the size limit, deserialize the strict versioned contract, validate cross-field requirements and embedded template fallback, and return the canonical project payload plus resolved absolute source paths. Accept but do not otherwise consume the reserved optional `preview.png` entry.
- `write_project_file(path, project_json)`: deserialize and validate `project_json` before creating an archive, write it as the required root entry, finish the ZIP, and atomically replace the target only after the complete archive succeeds. The initial implementation writes no preview entry.

Define Rust project DTOs with Serde tagged enums and `deny_unknown_fields`. Reuse `ovrley_core::commands::validate_template_contents` or an extracted durable-template validator for the embedded fallback instead of implementing a second template normalizer. Keep ZIP I/O, archive entry validation, and path resolution/conversion at this native filesystem boundary so frontend consumers never infer archive or platform path semantics.

Expose `readProjectFile` and `writeProjectFile` wrappers from `app/src/api/backend.js`. The frontend continues to exchange the structured project payload/JSON with these commands and never opens the ZIP itself. The wrappers should only translate Tauri command errors to `Error`; they must not add archive handling, fallback parsing, or field coercion.

## Implementation sequence

### 1. Characterize and define the contract

1. Add the pinned `zip` dependency, project/archive constants, and Rust DTOs.
2. Add one valid contract round-trip test and a compact table covering malformed JSON, wrong format/version, and one representative invalid required field.
3. Add one snapshot test proving that activity/video entries contain only locators and that representative parsed/runtime/editor fields are absent.
4. Add one path-locator test covering a project-relative child and an absolute external file.
5. Document the version 1 example near the implementation as the authoritative disk shape.

### 2. Correct source identity at existing owners

1. Change activity ingress so native selections carry their original absolute path through GPX, FIT, SRT, IGC, CSV, and VBO parsing. Expose parse-only functions that return `{ source, parsedActivity }`; activation remains a separate existing-session operation.
2. Replace template filename/source pairs with the canonical bundled/file source descriptor. Every user-template load or save must retain the exact absolute source path in application state, including files under the normal OVRLEY template directory. Update backend template listing/loading results so user templates expose their actual path at the owner boundary; do not reconstruct that path from a `user:*` identifier in project code.
3. Extract a parse/import-only video preparation function from `useVideoImport`. On every invocation it must read and probe the source video and extract telemetry from the file, returning fresh runtime data without mutating Zustand. It must not accept saved metadata or telemetry as inputs. Preview registration remains a runtime step performed only after all required staging succeeds.
4. Keep separate-activity precedence explicit: staged video telemetry is stashed when a separately loaded activity source exists; otherwise it becomes active.
5. Reuse the existing import-boundary tests. Update them only where extracting the parse-only seams changes their public interface; do not add project-specific format matrices.

### 3. Expose project-owned state at its existing domain owners

1. Add the canonical timeline viewport to `createEditorSlice` beside the existing playhead state and expose strict viewport actions through selector hooks. Do not move or otherwise change canvas zoom, background mode, grid, snap, mute, interpolation, or selection ownership.
2. Refactor `useTimelineViewport` and `useOverlayPlayer` to consume/store the canonical viewport. Preserve existing media-identity reset during ordinary imports, but allow project hydration to set the restored viewport after media reconstruction. Derive the displayed fit target from the range as today.
3. Add `createRenderSettingsSlice` and refactor render consumers around its canonical `renderSettings` aggregate. Opening the render dialog clones committed settings into a draft; Cancel discards the draft; Confirm validates availability, commits the accepted settings, and starts rendering.
4. Remove platform codec rewriting from project hydration. Move unavailable-codec handling to render-dialog initialization/submission, while keeping the saved identifier visible and unchanged until the user confirms another choice.
5. Add project lifecycle identity/baseline state locally in `useProjectLifecycle`; do not register it in Zustand.
6. Update undo partialization only for domain values that should remain editor-undoable. Keep project identity/baseline and runtime media reconstruction out of history.

### 4. Implement the save lifecycle

1. Add `createProjectSnapshot(state, projectPath)` as an explicit projection; never spread the store or serialize actions.
2. Before saving, require a canonical saved template source and `templateManagement.status === "Saved"`. If the template is Draft/Modified or has no resolvable source, stop project save and direct the user through the existing template Save flow; resume project save only after that flow returns a canonical descriptor.
3. Refresh `template.fallback` from the saved canonical template state, not an uncommitted dialog draft or stale fallback.
4. Implement Save:
   - If `loadedProjectPath` exists, write there.
   - Otherwise delegate to Save As.
5. Implement Save As with the native `.oly` picker and a filename derived from the current template/project name.
6. After a successful write, set the hook-local `loadedProjectPath` and derive `lastSavedProjectState` from the exact canonical payload that was written. Do not change them on cancellation or failure.
7. Relinked sources become part of the current in-memory project snapshot and mark the project modified until the user saves; do not silently rewrite the opened file.

### 5. Implement staged open and recovery

Implement `handleOpenProject()` as the picker entry point and `openProject(path)` as the staged state machine, both owned by `useProjectLifecycle`:

1. Open the native `.oly` picker through `openSinglePath(..., { lastDirectoryKey: LAST_PROJECT_OPEN_DIR_KEY })`. This reuses the same persisted-directory behavior as activity and video import. If the user cancels, stop without changing the remembered directory or current session. After a path is selected, read and validate the project through `read_project_file`; no editor/store state changes occur.
2. Resolve `template.source`:
   - Bundled template: load it through the bundled-template backend boundary and normalize through the existing template ingress.
   - User file template: resolve its saved path locator, read that exact file, and normalize it through the same template ingress, regardless of whether the path is inside or outside `Documents/OVRLEY`.
   - If missing, pause staging and show `MissingTemplateDialog` with exactly Locate, Use Embedded Fallback, and Cancel.
   - Locate validates the selected file as a template and updates only the staged descriptor.
   - Fallback uses the already-validated embedded durable template state and marks the staged template source as unresolved so the next project save requires saving it canonically first.
   - Cancel ends the operation without touching the current session.
3. Resolve each present activity/video locator. A missing source pauses staging in `MissingSourceDialog`; a selected replacement must pass the same extension/type ingress as the original source role. Cancel preserves the current session.
4. If an activity locator is present, always read and parse that file through the extracted existing format-specific parse-only boundary and canonical finalizer. The only project input to this operation is the resolved source path; no parsed activity or activity metadata exists in the project payload.
5. If a video locator is present, always perform a fresh video import from the resolved path: validate/read the file, probe it for current metadata, extract embedded telemetry through the existing native boundary, and prepare a new preview registration. The only project input to this operation is the resolved source path; no video metadata, telemetry, or preview state exists in the project payload. Treat absence of embedded telemetry as valid. Preserve the existing documented non-fatal telemetry-extraction behavior, but fail staging on required video read/probe/import errors.
6. Build the target session using fresh activity/video metadata plus saved sync, render, playhead, and timeline values. Clamp the valid saved viewport/playhead/export range to documented reconstructed-media bounds only where the PRD explicitly permits clamping. Carry no saved editor presentation or selection state because none exists in the contract.
7. Register the staged video with the preview server immediately before the store commit. `set_video` validates before replacing the current server registration; if registration fails, abort without changing the store. If there is no staged video, clear the old registration as part of the successful commit boundary.
8. Call `replaceEditorDocument(useStore, () => commitProjectHydration(target))` once. The utility must perform one store transaction that sets playback to paused, clears transient sync preview/warnings, clears processing/render state and errors, sets fresh preview identifiers/metadata, restores separate-activity precedence, and restores only the project-owned sync/render/playhead/timeline fields. It must leave canvas zoom, background mode, grid, snap, mute, interpolation preference, widget selection, and imported background image unchanged except for any existing template-load invariant needed to prevent an invalid selection.
9. Clear undo/redo history through `replaceEditorDocument`. Derive the hook-local `lastSavedProjectState` from the validated payload read from disk. Put accepted relinks or fallback recovery only in the current domain state so the project becomes Modified until the user explicitly saves those repairs.

If a theoretically fallible runtime cleanup remains after commit, make it idempotent and report it without rolling back durable state. All required validation and reconstruction must remain before the commit.

### 6. Add the Projects drawer

1. Add `PROJECTS_TOOL = "projects"` to `createLayoutSlice` and include it in drawer preference validation/migration.
2. Add a Projects icon and label to `VerticalToolbar`.
3. Add and export `ProjectsDrawerContent` from `app/src/features/toolbar/index.js`.
4. Keep `ProjectsDrawerContent` presentational. It receives current project name/path, project status, busy state, and `onOpen`, `onSave`, and `onSaveAs` callbacks.
5. Render these controls in the drawer:
   - Current project filename, or `Unsaved project`.
   - Status badge: `Saved`, `Modified`, or `Unsaved`.
   - Primary `Open Project` action.
   - `Save Project` action, disabled only while a project operation is running or required project prerequisites are unavailable.
   - Secondary `Save Project As` action.
   - Concise prerequisite/recovery text when the template must be saved first.
6. Compose `useProjectLifecycle` in `useAppShellComposition`, select `ProjectsDrawerContent` in `App.jsx`, and mount recovery dialogs at the shell level so they remain available if the drawer closes.
7. Disable conflicting project operations while a project load/save, activity import, video import, or render submission is active. Do not serialize those busy flags.

### 7. Integrate dirty state and application behavior

1. Add `useProjectSaveStatus` that compares only the current dirty-comparison projection with the hook-local `lastSavedProjectState`.
2. Ensure template edits can make both statuses meaningful: the template can be Modified while the project remains unchanged until the canonical template is saved; project saving is blocked until the template is Saved.
3. Project-owned timeline, playhead, render, sync, and source changes mark only the project Modified. Canvas/editor presentation and widget selection changes do not affect project status.
4. Template selection changes update the project projection's template source and fallback eligibility without treating editor presentation values as template or project content.
5. Keep startup behavior unchanged: restore the last template, but do not auto-open the last project, add autosave, or add crash recovery.

## File impact map

Expected new files:

- `app/src/features/projects/**`
- `app/src/features/toolbar/components/ProjectsDrawerContent.jsx`
- `app/src/store/slices/createRenderSettingsSlice.js`
- A small focused project test set covering the four essential frontend cases below
- `src-tauri/src/project_file.rs`

Expected modified seams:

- `app/src/App.jsx`
- `app/src/api/backend.js`
- `app/src/features/app-shell/hooks/useAppShellComposition.js`
- `app/src/features/player/hooks/useTimelineViewport.js`
- `app/src/features/player/hooks/useOverlayPlayer.js`
- `app/src/features/render-video/hooks/useRenderWorkflow.js`
- `app/src/features/template-manager/hooks/useTemplateManagement.js`
- `app/src/features/video-preview/hooks/useVideoImport.js`
- `app/src/hooks/useAppStoreSelectors.js`
- `app/src/lib/activity/import-activity.js`
- `app/src/store/slices/createEditorSlice.js`
- `app/src/store/slices/createLayoutSlice.js`
- `app/src/store/slices/createMediaSlice.js`
- `app/src/store/slices/createTemplateSlice.js`
- `app/src/store/useStore.js`
- `app/src/features/toolbar/components/VerticalToolbar.jsx`
- `app/src/features/toolbar/hooks/useDrawerPreference.js`
- `app/src/features/toolbar/index.js`
- `app/src/features/undo-redo/undoHistory.js`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/src/lib.rs`

Avoid a broad `App.jsx` project implementation. It should only compose the project hook, choose the active drawer content, and mount project dialogs.

## Test plan

Keep new coverage focused on the project boundaries. Existing template, parser, video-import, render, timeline, and undo tests remain responsible for their internal behavior.

### Essential frontend tests

1. **Projection contract:** one representative save proves the payload contains the template/source locators, sync/render/playhead/timeline fields, and no parsed activity, video metadata/telemetry/preview state, background image, or general editor settings.
2. **Successful lifecycle:** one orchestrator test opens a project with mocked native boundaries and proves the activity parser and fresh video import/telemetry boundaries are called with paths, the template is loaded from its saved path, the staged state commits once, playback is paused, the timeline/playhead are restored, and undo history is cleared.
3. **Atomic failure and recovery:** one table-driven orchestrator test covers malformed project rejection, a parser/probe failure, missing-template cancellation, and explicit fallback selection. Failure/cancellation must preserve the existing session; fallback must not occur silently.
4. **Drawer and remembered directory:** one UI/hook test proves Open/Save/Save As are routed correctly and the Open picker uses `LAST_PROJECT_OPEN_DIR_KEY`, updates it after selection, and leaves it unchanged on cancellation.

Add a focused render-workflow assertion only if the render-settings ownership refactor is not already covered by its existing tests. Do not duplicate every activity format, codec, template source, timeline fit target, editor setting, or relink permutation in project tests.

### Essential Rust tests

1. **Project file boundary:** a valid `.oly` archive containing `project.json` reads/writes successfully; a small table rejects an invalid ZIP, a missing `project.json`, malformed project JSON, and an unsupported version before replacing the target.
2. **Path resolution:** one project-relative child path and one absolute external path resolve correctly.

Rely on the existing template validator and preview-server tests instead of repeating their full matrices in `project_file` tests.

## Verification order

Run focused tests after each phase, then the complete supported suites:

1. `pnpm --dir app exec vitest run <focused project/import/store test files>`
2. `cargo test --manifest-path src-tauri/Cargo.toml project_file`
3. `pnpm --dir app exec vitest run`
4. `cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml`
5. `cargo test --manifest-path src-tauri/Cargo.toml`
6. `pnpm lint`

Do not run `pnpm build`, `pnpm tauri build`, or any build wrapper without explicit user permission.

## Completion criteria

The work is complete when a user can open the Projects drawer, save the project-owned sources/settings plus playhead/timeline range as `project.json` inside a small `.oly` ZIP container, close/restart OVRLEY, open that project, and have activity and video parsed again from disk before receiving the reconstructed template/media session paused at the saved playhead. The container reserves optional `preview.png` support without making it authoritative. Canvas zoom, background mode, grid, snap, mute, interpolation, widget selection, activity/video metadata, and parsed data are never serialized. Existing template changes flow into the project automatically, explicit fallback recovery works, project and template dirty indicators remain independent, and every failed/cancelled pre-commit load leaves the previous session usable and unchanged.

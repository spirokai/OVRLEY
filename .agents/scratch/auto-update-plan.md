# Installer Packaging and Automatic Updates — Implementation Plan

## Goal

Add installer packaging and signed automatic updates while preserving the existing release workflows as backups.

The new release system consists of three independently dispatchable OS workflows and one manual orchestrator:

- `.github/workflows/workflow_win.yml`
- `.github/workflows/workflow_macos.yml`
- `.github/workflows/workflow_linux.yml`
- `.github/workflows/workflow_release.yml`

The orchestrator runs all three OS workflows in parallel, collects their Actions artifacts, creates a complete `latest.json`, uploads the final release assets to one draft GitHub Release, validates it, and publishes it.

## Decisions and Constraints

- Keep all existing `release-*` workflows unchanged as backups.
- Do not add an Intel macOS workflow or `darwin-x86_64` updater entry.
- The orchestrator is manual-only.
- Stable semantic versions only are supported (`X.Y.Z`).
- Semantic Release remains the owner of release tag creation. Its existing tag format is the normalized version without a `v` prefix.
- The new workflows discover the Semantic Release tag pointing at the selected commit, as the current release workflows do. They do not create tags.
- An independently dispatched OS workflow creates the matching draft GitHub Release if it does not exist and uploads that OS's artifacts to it.
- An independently dispatched OS workflow may update an existing release only while it is a draft. It must reject a published release for the same tag.
- The orchestrator uses Actions artifacts between build jobs and the publishing job; its called OS workflows must not perform intermediate GitHub Release uploads.
- The orchestrator may update an existing release only while it is a draft. It must reject a published release for the same tag.
- The orchestrator publishes only after the complete local artifact set, signatures, manifest, and draft release asset inventory validate.
- Update checks run once during application startup and do not block startup.
- A failed update check, such as while offline, is logged and otherwise silent.
- A failure after the user starts an update displays `Update failed`; the user can close the dialog and continue using the installed version.
- Portable Windows builds do not check for updates while their `.ovrley-portable` marker is present.
- Cross-version updater acceptance testing on real Windows, macOS, and Linux systems is manual and will be performed by the project owner.
- Never run a production build during implementation without explicit user permission.

## Final Published Release Assets

The orchestrated GitHub Release contains exactly these six public assets:

1. `OVRLEY-windows-<version>.exe`
2. `OVRLEY-windows-<version>-portable.zip`
3. `OVRLEY-macos-<version>.dmg`
4. `OVRLEY-macos-<version>.app.tar.gz`
5. `OVRLEY-linux-<version>.AppImage`
6. `latest.json`

The generated `.sig` files are required build intermediates. They are transferred through Actions artifacts and consumed while generating `latest.json`, but are not uploaded as public GitHub Release assets. Their complete file contents are embedded in the corresponding manifest entries.

There is no MSI, Debian package, Intel macOS package, standalone `.app`, separate Windows updater ZIP, or publicly uploaded `.sig` file.

The macOS `.app.tar.gz` is required in addition to the DMG: the DMG is the user-facing initial installer, while `.app.tar.gz` is Tauri's macOS updater payload. The Windows setup executable and Linux AppImage each serve as both the user-facing package and updater payload.

## 1. Canonical Release Versioning

Add a strict release-version script, for example `scripts/set-release-version.mjs`.

The script must:

- Accept exactly one version value.
- Require stable SemVer in `X.Y.Z` form.
- Reject prerelease, build metadata, partial, empty, or malformed versions.
- Treat the Semantic Release tag and normalized version as the same value because the repository's `tagFormat` is `${version}`.
- Update only the ephemeral checked-out workflow workspace; never commit or push version changes.
- Update:
  - `package.json`
  - `app/package.json`
  - `src-tauri/tauri.conf.json`
  - `src-tauri/Cargo.toml`
- Update `src-tauri/Cargo.lock` as required after changing the application crate version.
- Re-read every version source and fail unless all values exactly match.
- Print or expose the normalized version for subsequent workflow steps.

Every new workflow must:

1. Check out the selected ref with full tag history.
2. Resolve the stable Semantic Release tag pointing at the checked-out `HEAD`, using the same ownership model as the current workflows.
3. Fail if no stable tag points at `HEAD` or if more than one candidate makes the release identity ambiguous.
4. Record and log the exact commit SHA and release tag.
5. Run the release-version script before the Tauri build.
6. Pass the same normalized value to `VITE_OVRLEY_VERSION`.
7. Verify the packaged native Tauri version against the release tag after building.

Do not rely on `VITE_OVRLEY_VERSION` as the native updater version. Tauri's version in `src-tauri/tauri.conf.json` must match the release tag.

## 2. Tauri Updater Integration

Add matching Tauri v2 updater dependencies:

- Rust:
  - `tauri-plugin-updater`
  - `tauri-plugin-process`
- Frontend:
  - `@tauri-apps/plugin-updater`
  - `@tauri-apps/plugin-process`

Update `src-tauri/src/lib.rs` to initialize both plugins once on the application builder.

Update `src-tauri/capabilities/default.json` with the minimum updater and process-relaunch permissions needed by the frontend. Do not grant unrelated process capabilities.

Update `src-tauri/tauri.conf.json` to configure:

- `bundle.createUpdaterArtifacts: true`
- The updater public key content
- The production HTTPS endpoint:
  - `https://github.com/spirokai/cyclemetry-reloaded/releases/latest/download/latest.json`
- The intended Windows updater install mode, using Tauri's default/recommended passive behavior unless implementation testing shows a product reason to change it

Store only the public updater key in repository configuration. Store the private signing material only in GitHub Actions secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Pass both secrets only to jobs that build signed updater artifacts. Document that the private key requires a secure offline backup because losing it prevents installed versions from trusting future updates.

## 3. Canonical Distribution Detection

Introduce one Rust-owned distribution model with exactly two serialized values:

- `installed`
- `portable`

Detection rules:

- On Windows, inspect for `.ovrley-portable` beside the current executable once during Rust startup.
- Marker present: `portable`.
- Marker absent: `installed`. Marker absence is documented optional absence, not malformed state.
- On macOS and Linux, always use `installed` for updater eligibility. A similarly named marker on those platforms must not disable updates.
- Store the detected enum in managed Tauri application state.
- Expose one strict Tauri command returning the canonical distribution value.
- Add the command to the existing invoke handler and the canonical frontend backend API boundary.
- Do not expose paths or make the frontend repeat filesystem detection.

Keep distribution detection separate from packaged-resource resolution. The existing runtime resource-root behavior must continue to locate adjacent portable resources and Tauri bundle resources correctly.

## 4. Shared Packaging Resources

Refactor the reusable resource and document generation currently embedded in `scripts/package-portable.mjs` into a dedicated module or pre-build script. Keep one owner for generated content; workflows and packagers must not duplicate the text.

Preserve:

- `fonts/`
- `templates/`
- `vendor/ffmpeg/`
- `THIRD_PARTY_NOTICES.txt`
- The existing macOS and Linux `INSTALL.txt` content, adjusted only where the new package format makes an instruction inaccurate
- Existing FFmpeg `-version` and `-L` output in the third-party notice

The current Windows portable archive does not contain an `INSTALL.txt`; do not invent a new Windows installation document unless separately requested.

The shared generator must run before `tauri build` so Tauri can include generated documents as bundle resources. Update `src-tauri/tauri.conf.json` resource mappings so the following are present in every applicable packaged application:

- Fonts
- Templates
- Self-contained FFmpeg and FFprobe command-line binaries
- `THIRD_PARTY_NOTICES.txt`
- Platform `INSTALL.txt` where one currently exists

Expected placement:

- NSIS installation: Tauri resource directory
- macOS: `OVRLEY.app/Contents/Resources`
- Linux AppImage: packaged Tauri resource directory
- Windows portable ZIP: archive root using the existing `fonts/`, `templates/`, and `vendor/ffmpeg/` layout

Make the third-party notice wording valid for both installed and portable distributions while preserving its licensing substance and generated FFmpeg output.

Refactor `scripts/package-portable.mjs` to consume the shared generated resources rather than regenerate notices independently. Preserve its review-friendly structure and remove only responsibilities transferred to the shared generator.

## 5. Windows Workflow

Add `.github/workflows/workflow_win.yml` with both `workflow_dispatch` and `workflow_call` triggers.

Inputs and behavior must be defined consistently for both triggers. In called/orchestrated mode, the orchestrator supplies the selected ref and resolved release identity. In independent mode, the workflow resolves the Semantic Release tag at the selected checked-out ref using the canonical versioning step.

Build requirements:

- Runner: `windows-latest`
- Architecture: x86_64
- Bundle target: NSIS only
- Enable updater artifact creation and signing
- Build the application executable once
- Use that same compiled executable for both the NSIS installer and portable ZIP
- Never compile a separately configured portable executable

Normalize outputs to:

- `OVRLEY-windows-<version>.exe`
- Its temporary `.sig`
- `OVRLEY-windows-<version>-portable.zip`

Portable staging must:

- Copy the exact executable used by the NSIS build
- Preserve the current portable resource layout
- Add `.ovrley-portable` only to the portable staging directory
- Add no marker to Tauri resources or installer inputs

Verify before upload:

- Exact artifact names
- Setup executable and signature exist and are non-empty
- Portable ZIP contains `OVRLEY.exe`
- Portable ZIP contains `.ovrley-portable` at the expected root
- Portable ZIP contains fonts, templates, FFmpeg, FFprobe, and third-party notices
- The staged NSIS input/resources do not contain `.ovrley-portable`
- The portable and installer paths originate from the same compiled executable; compare hashes before installer packaging where possible, or otherwise validate the retained build executable against portable staging before it is renamed/zipped
- Native version equals the release tag

Publishing behavior:

- Under `workflow_call`: upload the normalized setup executable, portable ZIP, and setup signature as one Actions artifact; do not create or update a GitHub Release.
- Under `workflow_dispatch`: create the release as a draft if absent, or update it only if it remains a draft; reject an already published release. Upload the setup executable and portable ZIP as public release assets. Do not upload the signature publicly.

## 6. macOS Workflow

Add `.github/workflows/workflow_macos.yml` with both `workflow_dispatch` and `workflow_call` triggers.

Build requirements:

- Use a pinned standard Apple Silicon runner label rather than an Intel runner or an architecture-ambiguous custom runner.
- Target `aarch64-apple-darwin` only.
- Use ad-hoc signing identity `-`.
- Bundle the DMG and enable updater artifact generation.
- Do not build Intel, universal, or standalone portable ZIP outputs.

Normalize outputs to:

- `OVRLEY-macos-<version>.dmg`
- `OVRLEY-macos-<version>.app.tar.gz`
- Its temporary `.sig`

Verify before upload:

- Exact artifact names
- DMG, updater archive, and signature exist and are non-empty
- The built `.app` has the expected aarch64 executable
- The app bundle contains FFmpeg, FFprobe, fonts, templates, notices, and macOS installation documentation
- The updater archive contains the expected application bundle and resources
- Native bundle version equals the release tag
- Ad-hoc signing was applied as intended

Publishing behavior:

- Under `workflow_call`: upload the normalized DMG, updater archive, and signature as one Actions artifact; do not create or update a GitHub Release.
- Under `workflow_dispatch`: create the release as a draft if absent, or update it only if it remains a draft; reject an already published release. Upload the DMG and updater archive as public release assets. Do not upload the signature publicly.

## 7. Linux Workflow

Add `.github/workflows/workflow_linux.yml` with both `workflow_dispatch` and `workflow_call` triggers.

Build requirements:

- Runner: exactly `ubuntu-22.04`
- Architecture: x86_64
- Bundle target: AppImage only
- Enable updater artifact creation and signing
- Use the non-shared BtbN GPL build for self-contained FFmpeg and FFprobe binaries
- Stop producing a Debian package in the new workflow

Normalize outputs to:

- `OVRLEY-linux-<version>.AppImage`
- Its temporary `.sig`

Verify before upload:

- Exact artifact names
- AppImage and signature exist and are non-empty
- AppImage executable bit is set
- Extracted AppImage contains FFmpeg, FFprobe, fonts, templates, notices, and Linux installation documentation
- Extracted FFmpeg and FFprobe are executable and run without a bundled shared-library directory
- Native version equals the release tag

Publishing behavior:

- Under `workflow_call`: upload the normalized AppImage and signature as one Actions artifact; do not create or update a GitHub Release.
- Under `workflow_dispatch`: create the release as a draft if absent, or update it only if it remains a draft; reject an already published release. Upload the AppImage as a public release asset. Do not upload the signature publicly.

## 8. Manual Release Orchestrator

Add `.github/workflows/workflow_release.yml` with `workflow_dispatch` only.

Inputs:

- The repository ref/branch to release, defaulting to the normal release branch where practical
- Release body/notes

The orchestrator does not accept or create an arbitrary version. It checks out the selected ref, fetches tags, and resolves the stable Semantic Release tag pointing at `HEAD`, matching the current release workflow model.

Before starting builds, it must:

- Resolve exactly one stable `X.Y.Z` tag at `HEAD`
- Record the exact commit SHA
- Reject missing, malformed, prerelease, or ambiguous tags
- Check whether a GitHub Release exists for the tag
- Continue if no release exists
- Continue if the existing release is a draft
- Fail without modifying anything if the existing release is published

Invoke the three OS workflows in parallel using reusable `workflow_call` jobs. Pass the same ref, tag, version, commit identity, and updater signing secrets to all three. Reusable workflows must validate received values instead of trusting them implicitly.

After all three succeed, the publishing job must:

1. Download all three Actions artifacts.
2. Flatten or normalize the download layout into one clean staging directory.
3. Reject unexpected files and duplicate artifact names.
4. Validate the exact expected public assets and temporary signatures.
5. Generate `latest.json` with the strict manifest script.
6. Validate the manifest locally.
7. Create the GitHub Release as a draft if absent, or reuse it only if it remains a draft.
8. Remove or replace stale assets in that draft whose names collide with this release's canonical assets, without touching unrelated releases.
9. Upload exactly the six final public assets.
10. Query the GitHub API with authentication and verify the draft release contains exactly the intended asset names, with non-zero sizes.
11. Verify every URL encoded in `latest.json` maps to the matching uploaded asset name and tag.
12. Publish the release.
13. Perform post-publication smoke checks for `releases/latest/download/latest.json` and every artifact URL in the manifest.

Public URL resolution cannot be fully proven while the release is a draft. Treat authenticated draft asset inventory validation as the publication gate, then perform public HTTP resolution checks immediately after publication.

The orchestrator must not publish if any build, signature, resource, version, manifest, or draft asset inventory check fails.

## 9. Strict Manifest Generation

Add `scripts/create-latest-json.mjs` as the only owner of updater manifest construction.

Inputs must be explicit, including:

- Repository owner/name
- Release tag
- Normalized release version
- Release notes/body
- Staging directory containing canonical artifacts and signatures
- Output path

The script must fail loudly unless all contracts hold:

- Version is stable `X.Y.Z` SemVer
- Release tag exactly matches the normalized version under the repository's Semantic Release tag format
- Required artifact filenames exactly match the canonical names
- Required signature filenames exist
- Signature files are non-empty after trimming
- No unsupported platform is included
- Platform keys are exactly:
  - `windows-x86_64`
  - `darwin-aarch64`
  - `linux-x86_64`
- Windows URL points to the NSIS setup executable
- macOS URL points to `.app.tar.gz`, not the DMG
- Linux URL points to the AppImage
- Every URL uses the supplied repository, literal tag, and normalized public filename
- Signature values contain file contents, never URLs or filenames
- Notes use the supplied GitHub Release body without hidden fallback text

Write deterministic formatted JSON so repeated generation from identical inputs produces identical output.

## 10. Frontend Update UX

Add the feature boundary:

- `app/src/features/app-update/hooks/useAppUpdate.js`
- `app/src/features/app-update/components/UpdatePromptDialog.jsx`
- An `index.js` export surface if consistent with neighboring features

The hook owns updater state and side effects. The dialog is controlled and presentational.

Integrate the hook into `useAppShellComposition.js` alongside `useAppBootstrap()` and return its canonical domain object unchanged. Render `UpdatePromptDialog` from `App.jsx`.

Startup behavior:

1. Read the canonical distribution kind from the Rust backend.
2. If it is `portable`, finish without importing/calling the updater plugin.
3. If it is `installed`, invoke one asynchronous update check during startup.
4. Do not block editor bootstrap, template loading, codec loading, or normal use.
5. Guard the effect lifecycle so remounts or development Strict Mode do not create concurrent duplicate checks.

State and behavior:

- No update: render nothing.
- Check failure/offline: log the failure, render nothing, and leave the application usable.
- Update available: show version and release notes with `Update now` and `Later`.
- Later: close the prompt and do not check again during that process lifetime.
- Update now: show determinate progress when total size is available and an indeterminate downloading state otherwise.
- Successful download/install: request relaunch through the process plugin when the platform update flow returns control and relaunch is required.
- Download, signature, installation, or relaunch failure: display `Update failed`, preserve useful diagnostic detail for logging, and allow the dialog to be closed so the old version remains usable.

Do not add dirty-editor-state handling in this iteration because update checks occur only during startup. Keep the confirmation explicit before starting the update, especially on Windows where the updater installer can terminate the application.

Do not add compatibility aliases, inferred distribution values, fallback versions, or frontend filesystem checks.

## 11. Minimum Release Checks

Do not add a new frontend, Rust, script, integration, or automated end-to-end test suite for this feature. The current release workflows do not run the repository's tests, and the new workflows do not need to introduce a general test stage.

Keep only the inline packaging checks required to avoid publishing unusable updater metadata or incomplete distributions:

- Required output files and signatures exist and are non-empty.
- The native version equals the Semantic Release tag.
- The Windows portable ZIP contains `.ovrley-portable`; installer inputs do not.
- The resources explicitly required by this feature are present in each package.
- `latest.json` contains exactly the three supported platforms, embeds the signatures, and points to the expected asset names.
- The draft release is still a draft and contains the expected six assets before publication.

These are workflow release gates, not a new automated test suite. Do not duplicate the same checks at multiple stages unless a later stage crosses a real boundary, such as verifying the uploaded GitHub asset inventory after local files were already checked.

Do not run the repository's production build command while implementing or locally checking this work without explicit user permission.

## 12. Manual Two-Version Updater Acceptance

The project owner will perform manual end-to-end updater testing. Implementation should provide a concise checklist but does not need to build automated GUI/update-channel infrastructure.

For Windows installed, Apple Silicon macOS, and Linux AppImage:

1. Install or run version `N` through the intended user-facing package.
2. Publish signed version `N+1` through a controlled test release path that exercises the real manifest format.
3. Launch version `N`.
4. Confirm the update prompt shows version `N+1` and its release notes.
5. Choose `Later` and confirm the application remains usable and does not prompt again during that launch.
6. Relaunch version `N`, choose `Update now`, and verify download progress.
7. Confirm signature verification, installation, and relaunch complete.
8. Confirm the resulting native application version is `N+1`.
9. Repeat with the machine offline and confirm a failed check is invisible to the user.
10. Exercise an invalid or unavailable updater payload and confirm `Update failed` can be dismissed while version `N` remains usable.

For the Windows portable ZIP:

1. Extract the archive and verify `.ovrley-portable` is beside `OVRLEY.exe`.
2. Launch it while an update is available.
3. Confirm no updater request or prompt occurs.
4. Confirm normal rendering resources and FFmpeg operation still work from the portable layout.

Manual acceptance results should be recorded before treating the updater channel as production-ready.

## Implementation Order

Implement in dependency order:

1. Strict versioning and manifest scripts.
2. Shared packaging-resource generation and resource mappings.
3. Rust updater plugins, capabilities, updater configuration, and distribution detection.
4. Frontend update hook, dialog, and integration.
5. Windows workflow and validations.
6. macOS ARM workflow and validations.
7. Linux AppImage workflow and validations.
8. Orchestrator, draft-release safeguards, manifest assembly, publication gate, and public smoke checks.
9. Static review and the minimum release checks from section 11.
10. User-authorized real builds followed by owner-operated manual two-version acceptance.

## Completion Criteria

The feature is complete when:

- All four new workflows exist and the old `release-*` workflows are unchanged.
- Each OS workflow can run independently and populate a matching draft release without modifying a published release.
- The orchestrator builds all three platforms in parallel and is the only workflow that publishes a completed release.
- The published release contains exactly the six canonical assets.
- Installed Windows, macOS ARM, and Linux builds use signed automatic updates.
- The Windows portable build uses the same compiled executable, contains `.ovrley-portable`, and skips updater checks.
- Fonts, templates, FFmpeg, notices, and applicable installation documentation remain packaged.
- Offline update checks fail silently.
- User-initiated update failures show a dismissible `Update failed` state.
- The minimum release checks pass and the project owner completes manual two-version updater acceptance.

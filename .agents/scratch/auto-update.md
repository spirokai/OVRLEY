# Installer Packaging and Automatic Updates

## Feasibility

This is achievable with Tauri v2 without Windows or Apple code-signing certificates. However, Tauri updater artifacts must still be cryptographically signed with a Tauri updater key. This is mandatory and separate from OS code signing.

Recommended distribution set:

| Platform | User-facing package | Updater artifact |
| --- | --- | --- |
| Windows x64 installed | NSIS `setup.exe` | Same `setup.exe` plus `.sig` |
| Windows x64 portable | `.zip` containing the same executable plus `.ovrley-portable` | Updates disabled |
| macOS Apple Silicon | `.dmg` | `.app.tar.gz` plus `.sig` |
| macOS Intel | `.dmg` | `.app.tar.gz` plus `.sig` |
| Linux x64 | `.AppImage` | Same `.AppImage` plus `.sig` |

Only Windows has separate installed and portable distributions. macOS has one DMG distribution containing an updateable `.app`, and Linux has one portable, updateable AppImage distribution.

## Current Blockers

1. Native versions are incorrect on Windows and macOS.

   `src-tauri/tauri.conf.json:4` remains `0.1.0`, while only `VITE_OVRLEY_VERSION` is set during those builds. Tauri's updater compares against the native application version, not the displayed Vite version.

2. Windows installers are explicitly disabled.

   `.github/workflows/release-win.yml:51-55` sets `bundle.targets` to an empty array and builds only the executable.

3. macOS only builds an application bundle.

   `.github/workflows/release-macos.yml:51-57` and `.github/workflows/release-macos-intel.yml:57-66` build `.app` for the portable archive, not a user-facing DMG.

4. Linux only builds `.deb`.

   `.github/workflows/release-linux.yml:115-150` does not create the AppImage needed for Tauri's normal Linux self-update path.

5. There is no updater integration.

   The updater and process plugins are absent from:

   - `src-tauri/Cargo.toml`
   - `src-tauri/src/lib.rs`
   - `src-tauri/capabilities/default.json`
   - `app/package.json`
   - `src-tauri/tauri.conf.json`

6. Releases remain drafts.

   All platform workflows upload to a draft GitHub release. `https://github.com/.../releases/latest/download/latest.json` only works with a published release.

7. Separate workflows cannot safely produce one update manifest.

   `latest.json` must contain every supported OS and architecture. Publishing it from one platform workflow could expose an incomplete release while other jobs are still running.

## Required Tauri Changes

Add the desktop updater and process plugins:

- Rust dependencies: `tauri-plugin-updater`, `tauri-plugin-process`
- Frontend dependencies: `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`
- Plugin initialization in `src-tauri/src/lib.rs`
- `updater:default` and process relaunch permissions in `src-tauri/capabilities/default.json`

Configure `src-tauri/tauri.conf.json` approximately as follows:

```json
{
  "bundle": {
    "createUpdaterArtifacts": true
  },
  "plugins": {
    "updater": {
      "pubkey": "TAURI UPDATER PUBLIC KEY CONTENT",
      "endpoints": [
        "https://github.com/spirokai/cyclemetry-reloaded/releases/latest/download/latest.json"
      ]
    }
  }
}
```

Production updater endpoints must use HTTPS.

## Updater Signing

Generate one updater key pair:

```sh
pnpm tauri signer generate -w ~/.tauri/ovrley.key
```

Store:

- Public key in `tauri.conf.json`
- Private key as GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY`
- Optional password as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- A secure offline backup of the private key

Every installer build must receive those variables. Losing the private key means existing installations cannot trust future updates.

This does not require a purchased certificate.

## Release Workflow

The most reliable design is one coordinating release workflow:

1. Resolve and validate the semantic version.
2. Set the same version for every native build.
3. Build Windows, macOS ARM, macOS Intel, and Linux in parallel.
4. Upload outputs as temporary GitHub Actions artifacts.
5. Have a final publishing job collect all artifacts.
6. Generate one complete `latest.json`.
7. Upload the Windows installer and portable archive, macOS DMGs, Linux AppImage, updater artifacts, signatures, and `latest.json` to one GitHub release.
8. Publish the release only after every required artifact and signature is present.

The manifest would contain entries such as:

```json
{
  "version": "1.2.3",
  "notes": "Release notes",
  "platforms": {
    "windows-x86_64": {
      "url": ".../OVRLEY-windows-x86_64-setup.exe",
      "signature": "contents of the .sig file"
    },
    "darwin-aarch64": {
      "url": ".../OVRLEY-macos-aarch64.app.tar.gz",
      "signature": "..."
    },
    "darwin-x86_64": {
      "url": ".../OVRLEY-macos-x86_64.app.tar.gz",
      "signature": "..."
    },
    "linux-x86_64": {
      "url": ".../OVRLEY-linux-x86_64.AppImage",
      "signature": "..."
    }
  }
}
```

The signature value must be the contents of the `.sig` file, not its URL.

The existing workflows could be converted to reusable `workflow_call` jobs, or replaced by a matrix-style release workflow. Keeping independent manually dispatched workflows plus a separate manual "finalize release" workflow is possible but more error-prone.

## Distribution Model

Windows should use one canonical installer technology for updating. NSIS is recommended:

- Per-user installation by default
- No administrator requirement
- Straightforward passive updater installation
- Produces a conventional `setup.exe`

MSI can also be published, but the update manifest can only select one Windows artifact per architecture. Mixing MSI and NSIS installation lifecycles can cause registration or duplicate-installation problems.

Windows also retains a portable ZIP. It contains the same compiled executable as the NSIS package, but the portable staging directory contains a `.ovrley-portable` marker that disables update checks. The executable therefore only needs to be compiled once.

macOS publishes one DMG per supported architecture. The DMG contains the `.app` users copy into `/Applications`, while the updater uses the generated `.app.tar.gz`. The installed application and updater archive come from the same application build.

Linux publishes one AppImage. AppImage is portable and is also Tauri's standard self-updating Linux format, so no separate installer or `.deb` channel is needed.

Final release formats:

| Platform | Distribution | Update behavior |
| --- | --- | --- |
| Windows | NSIS installer | Checks, downloads, and installs signed updates |
| Windows | Portable ZIP | Does not check while `.ovrley-portable` is present |
| macOS | DMG containing `.app` | Checks and updates the app bundle |
| Linux | AppImage | Checks and updates the AppImage |

## Application UX

A new update feature should be owned by a hook, keeping the dialog presentational:

- `app/src/features/app-update/hooks/useAppUpdate.js`
- `app/src/features/app-update/components/UpdatePromptDialog.jsx`

Integration points:

- Start the check alongside `useAppBootstrap()` in `useAppShellComposition.js:48`
- Return the update domain object from the composition hook
- Render the controlled dialog in `App.jsx:67-81`

Suggested behavior:

1. Start the update check asynchronously after application startup.
2. Do not block the editor while checking.
3. If no update exists, show nothing.
4. If checking fails, log it without turning a temporary network failure into a blocking application error.
5. If an update exists, display the version and release notes.
6. Offer `Update now` and `Later`.
7. Show download progress after confirmation.
8. Install and relaunch after download.
9. Show an actionable error if download, verification, or installation fails.

On Windows, installing an update automatically exits the app due to installer limitations. The user should therefore confirm before download/install, and any unsaved editor state must be handled before installation begins.

## Windows Portable Marker

The Windows executable determines its distribution mode at runtime from a marker in its application directory:

```text
OVRLEY/
|-- OVRLEY.exe
|-- .ovrley-portable
|-- fonts/
|-- templates/
`-- vendor/
```

The packaging sequence is:

1. Compile the Windows application once with updater support.
2. Build the NSIS installer and signed updater artifact from that executable.
3. Copy the same executable and resources into portable staging.
4. Add `.ovrley-portable` to the staging directory.
5. Create the portable ZIP.

Rust should detect the marker once at startup and expose the canonical distribution kind to the update hook. The frontend should not infer portability from paths or repeat filesystem checks.

When `.ovrley-portable` exists, startup update checks are disabled. If a user deletes the marker, OVRLEY behaves like the installed distribution and can offer a signed NSIS update. Accepting that update effectively transitions the portable copy to an installed OVRLEY distribution. This is not an update-integrity risk because Tauri still requires a valid updater signature, but the behavior should be documented.

An environment variable is not sufficient by itself because it is not persisted by a ZIP and would require a launcher on every start. The marker preserves one executable while avoiding a launcher and a second compilation.

## Unsigned Distribution Consequences

### Windows

- Installers show `Unknown publisher`.
- Microsoft Defender SmartScreen may warn or block less-established downloads.
- Warnings may improve with download reputation but do not disappear reliably.
- Per-user NSIS installation avoids most elevation requirements.

### macOS

- Gatekeeper normally evaluates OVRLEY when the user first launches the downloaded app, not while opening the DMG or copying the app into `/Applications`.
- The browser-applied quarantine attribute follows the app from the downloaded DMG to the copied application.
- Because OVRLEY is not Developer ID signed and notarized, the first launch can show an unidentified-developer warning or be blocked.
- On Apple Silicon, use Tauri's ad-hoc signing identity `"-"`. This requires no certificate but does not establish developer trust.
- Users may use right-click `Open`, approve OVRLEY under Privacy & Security, or remove quarantine with `xattr -cr /Applications/OVRLEY.app`.
- Tauri's updater signature verifies subsequent updates independently of Gatekeeper.
- A browser-style quarantine prompt is less likely for updater-downloaded replacements, but this must be verified on clean macOS machines and must not be assumed.
- Without Apple notarization, this remains a significant onboarding obstacle.

### Linux

- There is no equivalent mandatory centralized certificate.
- AppImage users must set the executable bit.
- Build on an old enough Linux baseline, ideally Ubuntu 22.04 or Debian 12, to avoid glibc incompatibility.

The Tauri updater signature still protects all platforms from modified or malicious update files.

## Testing Required

Automated frontend tests should cover:

- No update
- Update available
- User chooses later
- Successful download and installation
- Progress reporting
- Check failure
- Signature/download failure
- Relaunch behavior

Release validation should verify:

- Native version equals the release tag
- The Windows portable ZIP contains `.ovrley-portable`, while the NSIS installation does not
- Every manifest URL returns the expected artifact
- Every signature is present and embedded correctly
- FFmpeg and other resources remain bundled
- `latest.json` is not published before all platforms complete

End-to-end updater testing requires two real versions:

1. Install version `N`.
2. Publish `N+1` to a test release endpoint.
3. Launch `N`.
4. Verify prompt, download, installation, relaunch, and resulting native version on each OS.

## Recommended Scope

For the first iteration:

- Windows x64 NSIS installer and updater
- Windows x64 portable ZIP using `.ovrley-portable` to disable update checks
- macOS ARM64 and Intel DMGs with ad-hoc signing and updater archives
- Linux x64 AppImage with updater
- Consolidate publishing into one coordinated release workflow

The finalized model has one updateable format for macOS, one updateable format for Linux, and installed plus portable options for Windows. Only the Windows portable package disables update checks.

## References

- [Tauri updater plugin](https://v2.tauri.app/plugin/updater/)
- [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)
- [Tauri macOS DMG](https://v2.tauri.app/distribute/dmg/)
- [Tauri macOS signing and ad-hoc signing](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri Linux AppImage](https://v2.tauri.app/distribute/appimage/)

//! build.rs — patches telemetry-parser at compile time to expose DJI per-frame
//! camera metadata (ISO, shutter speed, white balance, f-number, focal length,
//! exposure value) that the upstream parser reads internally but doesn't insert
//! into the tag map.
//!
//! The patch lives at `patches/telemetry-parser-dji-camera-metadata.patch` and
//! is applied to cargo's git checkout of the pinned upstream commit. If the
//! upstream `src/dji/mod.rs` changes on upgrade, the patch will fail `--check`
//! and silently skip; a compile error on the unresolved `TagId` symbols will
//! signal that the patch needs updating.
//!
//! Once upstream merges equivalent functionality, this build script and the
//! patch file can both be deleted.

use std::path::Path;
use std::process::Command;

/// Applies the fitparser patch to cargo's registry copy, failing the build if it cannot.
fn patch_fitparser(manifest_dir: &Path, cargo_home: &str) {
    const PATCH: &str = "patches/fitparser-undeclared-developer-fields.patch";
    println!("cargo:rerun-if-changed={PATCH}");
    let patch_path = manifest_dir.join(PATCH);
    let registry = Path::new(cargo_home).join("registry/src");
    for index in std::fs::read_dir(registry).into_iter().flatten().flatten() {
        let source = index.path().join("fitparser-0.11.0");
        if !source.exists() {
            continue;
        }
        let git_apply = |args: &[&str]| {
            Command::new("git")
                .arg("apply")
                .args(args)
                .arg(&patch_path)
                .current_dir(&source)
                .output()
                .is_ok_and(|output| output.status.success())
        };
        assert!(
            git_apply(&["--reverse", "--check"]) || git_apply(&[]),
            "{PATCH} does not apply to {}",
            source.display()
        );
    }
}

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo_home =
        std::env::var("CARGO_HOME").unwrap_or_else(|_| {
            format!("{}/.cargo", std::env::var("HOME")
            .unwrap_or_else(|_| std::env::var("USERPROFILE")
            .unwrap_or_else(|_| ".".into())))
        });
    patch_fitparser(manifest_dir, &cargo_home);

    let patch_path = manifest_dir.join("patches/telemetry-parser-dji-camera-metadata.patch");
    if !patch_path.exists() {
        return; // Patch removed or build outside repo — nothing to do.
    }

    // Locate cargo's git checkout of telemetry-parser inside CARGO_HOME.
    let checkouts_dir = Path::new(&cargo_home).join("git/checkouts");
    let Ok(entries) = std::fs::read_dir(&checkouts_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let dir_name = entry.file_name();
        if !dir_name.to_string_lossy().contains("telemetry-parser") {
            continue;
        }
        let Ok(versions) = std::fs::read_dir(entry.path()) else {
            break;
        };

        for version in versions.flatten() {
            if !version.path().join("src/dji/mod.rs").exists() {
                continue;
            }
            // `git apply --check` exits 0 when the patch would apply cleanly
            // (i.e. it hasn't been applied yet), and non-0 when it's already
            // applied or the context no longer matches.
            let check = Command::new("git")
                .args(["apply", "--check", &patch_path.to_string_lossy()])
                .current_dir(version.path())
                .output();
            match check {
                Ok(output) if output.status.success() => {
                    // Patch not yet applied — apply it now.
                    let _ = Command::new("git")
                        .args(["apply", &patch_path.to_string_lossy()])
                        .current_dir(version.path())
                        .status();
                }
                _ => {} // Already applied, or context changed — skip.
            }
        }
        break; // Only process the first matching checkout directory.
    }

    // Re-run this script only when the patch file itself changes.
    println!("cargo:rerun-if-changed=patches/telemetry-parser-dji-camera-metadata.patch");
}

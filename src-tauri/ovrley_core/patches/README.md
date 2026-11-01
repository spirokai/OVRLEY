# Patches

## telemetry-parser-dji-camera-metadata.patch

**Why:** telemetry-parser reads DJI per-frame `FrameMetaOfCamera` protobuf fields internally but does not insert them into the tag map. This patch adds `insert_tag!()` calls that expose ISO, shutter speed, white balance, and (for WA530/OQ101) f-number, focal length, and EV as structured `TagId` values.

**Applied by:** `build.rs` at compile time against cargo's git checkout of the pinned upstream commit.

**Upgrade:** When bumping the `rev` in `Cargo.toml`, if `src/dji/mod.rs` changed upstream the patch will fail to apply cleanly. Fix by rebasing the patch onto the new commit via `git format-patch`/`git am`, or remove this patch once upstream merges equivalent functionality.

## fitparser-undeclared-developer-fields.patch

**Why:** fitparser 0.11.0 rejects the whole file when a message contains a developer field with no `field_description`, as in `tests/fixtures/activity/test-dive.fit`. The patch skips the bytes of those fields instead.

**Applied by:** `build.rs` at compile time against cargo's registry copy of the crate. The build fails if the patch does not apply. Cargo does not rebuild registry crates when their source changes, so a `fitparser` compiled before the patch was applied needs `cargo clean -p fitparser`.

**Upgrade:** When bumping `fitparser`, update the version in `build.rs` and rebase the patch onto `src/de/parser.rs`. Remove the patch once upstream handles these fields.

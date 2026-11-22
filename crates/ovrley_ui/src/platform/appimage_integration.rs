//! Adds the running AppImage to the user's application menu. An AppImage carries its desktop
//! entry and icons inside the image, where launchers cannot see them, so on every start this
//! copies them into the user's XDG data directory. The entry is named after the app identifier
//! because Wayland compositors match a window's app id to `<app id>.desktop` to find its icon.
use ovrley_services::runtime::APP_IDENTIFIER;
use std::path::{Path, PathBuf};

/// Registers the AppImage when the app runs from one. Failure only costs the menu entry, so it is
/// logged rather than stopping startup.
pub fn register() {
    let (Some(appimage), Some(appdir)) = (std::env::var_os("APPIMAGE"), std::env::var_os("APPDIR"))
    else {
        return;
    };
    if let Err(error) = install(Path::new(&appimage), Path::new(&appdir)) {
        log::warn!("Could not add the AppImage to the application menu: {error}");
    }
}

fn install(appimage: &Path, appdir: &Path) -> Result<(), String> {
    let data_dir = dirs::data_dir().ok_or("the user's data directory is unknown")?;
    let entry_name = format!("{APP_IDENTIFIER}.desktop");
    let bundled_entry = appdir.join("usr/share/applications").join(&entry_name);
    let entry = launch_from(
        &std::fs::read_to_string(&bundled_entry)
            .map_err(|error| format!("{}: {error}", bundled_entry.display()))?,
        appimage,
    );
    let target = data_dir.join("applications").join(&entry_name);

    // An AppImage manager such as Shelly names its entry after the bundled one. When that entry
    // already launches this file, the manager owns it and its icons, so neither is touched.
    let path = appimage.to_string_lossy();
    if std::fs::read_to_string(&target).is_ok_and(|current| {
        current != entry
            && current
                .lines()
                .any(|line| line.starts_with("Exec=") && line.contains(&*path))
    }) {
        return Ok(());
    }

    // Icons go first so the entry never appears without one.
    let icon_sizes = appdir.join("usr/share/icons/hicolor");
    for size in read_dir(&icon_sizes)? {
        let icon = size.join("apps/ovrley.png");
        if icon.is_file() {
            let size = size.file_name().ok_or("icon size directory has no name")?;
            let target = data_dir
                .join("icons/hicolor")
                .join(size)
                .join("apps/ovrley.png");
            write_if_changed(
                &target,
                &std::fs::read(&icon).map_err(|error| error.to_string())?,
            )?;
        }
    }

    write_if_changed(&target, entry.as_bytes())
}

/// Points the bundled entry, whose `Exec` names the binary inside the image, at the AppImage file.
/// `TryExec` makes launchers hide the entry once that file is deleted.
fn launch_from(entry: &str, appimage: &Path) -> String {
    let path = appimage.to_string_lossy();
    // Desktop Entry spec: quote the argument, backslash-escape `"`, `` ` ``, `$` and `\` inside the
    // quotes, then escape those backslashes again as string-value escapes; `%` is written `%%`.
    let mut exec = String::from("Exec=\"");
    for character in path.chars() {
        match character {
            '"' | '`' | '$' => exec.extend(['\\', '\\', character]),
            '\\' => exec.push_str("\\\\\\\\"),
            '%' => exec.push_str("%%"),
            _ => exec.push(character),
        }
    }
    exec.push('"');

    entry
        .lines()
        .filter(|line| !line.starts_with("TryExec="))
        .map(|line| {
            if line.starts_with("Exec=") {
                format!("TryExec={path}\n{exec}\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect()
}

fn read_dir(dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::read_dir(dir)
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.path()))
                .collect()
        })
        .map_err(|error| format!("{}: {error}", dir.display()))
}

fn write_if_changed(target: &Path, contents: &[u8]) -> Result<(), String> {
    if std::fs::read(target).is_ok_and(|current| current == contents) {
        return Ok(());
    }
    let parent = target.parent().ok_or("target path has no parent")?;
    std::fs::create_dir_all(parent)
        .and_then(|()| std::fs::write(target, contents))
        .map_err(|error| format!("{}: {error}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::launch_from;
    use std::path::Path;

    #[test]
    fn entry_launches_the_appimage_with_spec_escaping() {
        let entry = "[Desktop Entry]\nName=OVRLEY\nExec=OVRLEY\nIcon=ovrley\n";
        assert_eq!(
            launch_from(entry, Path::new("/home/a b/$OVRLEY 100%.AppImage")),
            "[Desktop Entry]\nName=OVRLEY\nTryExec=/home/a b/$OVRLEY 100%.AppImage\n\
             Exec=\"/home/a b/\\\\$OVRLEY 100%%.AppImage\"\nIcon=ovrley\n"
        );
    }
}

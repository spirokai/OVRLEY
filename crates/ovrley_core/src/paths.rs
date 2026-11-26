//! Application paths: fonts, templates, runtime debug and temp folders, and the render output
//! folder, computed once at startup.

use crate::error::{CoreError, CoreResult};
use std::fs;
use std::path::PathBuf;

/// The application's folders, computed once at startup from the repository root, the active
/// resource root and the OVRLEY user-data root.
#[derive(Clone, Debug)]
pub struct AppPaths {
    pub repo_root: PathBuf,
    pub font_dirs: Vec<PathBuf>,
    pub debug_render_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub bundled_templates_dirs: Vec<PathBuf>,
    pub user_templates_dir: PathBuf,
    pub downloads_dir: PathBuf,
}

impl AppPaths {
    pub fn from_repo_root(repo_root: PathBuf) -> Self {
        Self::from_runtime_roots(repo_root.clone(), repo_root, documents_ovrley_dir())
    }

    pub fn from_runtime_roots(
        repo_root: PathBuf,
        resource_root: PathBuf,
        user_data_root: PathBuf,
    ) -> Self {
        let downloads_dir = user_data_root.clone();
        let runtime_dir = user_data_root.join(".runtime");
        let font_dirs = vec![resource_root.join("fonts"), repo_root.join("fonts")]
            .into_iter()
            .filter(|path| path.is_dir())
            .collect();
        let debug_render_dir = runtime_dir.join("debug").join("timings");
        let temp_dir = runtime_dir.join("tmp");
        let bundled_templates_dirs =
            vec![resource_root.join("templates"), repo_root.join("templates")]
                .into_iter()
                .filter(|path| path.is_dir())
                .collect();
        let user_templates_dir = user_data_root.join("templates");

        Self {
            repo_root: resource_root,
            font_dirs,
            debug_render_dir,
            temp_dir,
            bundled_templates_dirs,
            user_templates_dir,
            downloads_dir,
        }
    }

    pub fn ensure_dirs(&self) -> CoreResult<()> {
        for dir in [
            &self.debug_render_dir,
            &self.temp_dir,
            &self.user_templates_dir,
            &self.downloads_dir,
        ] {
            fs::create_dir_all(dir).map_err(|error| CoreError::Io {
                path: dir.clone(),
                source: error,
            })?;
        }
        Ok(())
    }

    pub fn bundled_template_path(&self, filename: &str) -> Option<PathBuf> {
        self.bundled_templates_dirs
            .iter()
            .map(|dir| dir.join(filename))
            .find(|path| path.is_file())
    }

    pub fn user_template_path(&self, filename: &str) -> Option<PathBuf> {
        let path = self.user_templates_dir.join(filename);
        path.is_file().then_some(path)
    }
}

/// The OVRLEY documents folder: `%USERPROFILE%\Documents\OVRLEY` on Windows and
/// `$HOME/Documents/OVRLEY` elsewhere, under `.` when the home folder is unknown.
fn documents_ovrley_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("OVRLEY")
}

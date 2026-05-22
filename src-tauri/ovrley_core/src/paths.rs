//! Application path configuration and resolution.
//!
//! Owns: AppPaths struct, path construction, template path resolution,
//!        directory ensuring.
//! Does not own: runtime configuration (that's `config`), render parameters.
//!
//! Allowed dependencies: std, crate::error.
//! Forbidden dependencies: config, activity, render, encode, commands.
//!
//! This module lives in a neutral location because both `commands` and
//! `encode` need `AppPaths`. Placing it here breaks the circular-ish
//! dependency where `encode` had to import from `commands`.

use crate::error::{CoreError, CoreResult};
use std::fs;
use std::path::PathBuf;

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
        Self::from_roots(repo_root.clone(), repo_root)
    }

    pub fn from_resource_root(repo_root: PathBuf, resource_root: PathBuf) -> Self {
        Self::from_roots(repo_root, resource_root)
    }

    fn from_roots(repo_root: PathBuf, resource_root: PathBuf) -> Self {
        let downloads_dir = downloads_ovrley_dir();
        let runtime_dir = downloads_dir.join(".runtime");
        let font_dirs = vec![resource_root.join("fonts"), repo_root.join("fonts")]
            .into_iter()
            .filter(|path| path.is_dir())
            .collect();
        let debug_render_dir = if resource_root == repo_root {
            repo_root.join("debug").join("timings")
        } else {
            runtime_dir.join("debug").join("timings")
        };
        let temp_dir = runtime_dir.join("tmp");
        let bundled_templates_dirs =
            vec![resource_root.join("templates"), repo_root.join("templates")]
                .into_iter()
                .filter(|path| path.is_dir())
                .collect();
        let user_templates_dir = documents_ovrley_dir();

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

fn documents_ovrley_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("OVRLEY")
}

fn downloads_ovrley_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Downloads").join("OVRLEY")
}

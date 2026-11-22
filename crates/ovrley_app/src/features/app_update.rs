//! Port of `features/app-update`: the one update check at startup and the update the user starts
//! (`useAppUpdate`, `UpdatePromptDialog`'s values).
//!
//! `readUpdateMetadata` and `readDownloadEvent` validated what Tauri's plugin sent. Here
//! `ovrley_services::updater` hands over typed values, so nothing is left to validate.

use crate::i18n::{catalog, Locale};
use crate::lib::format::format_update_progress;
use crate::store::services::{AppUpdate, Services, UiPost};
use crate::store::Store;
use ovrley_services::distribution::DistributionKind;
use ovrley_services::updater::DownloadProgress;
use std::sync::{Arc, Mutex};

/// Where `app-update.versionNowAvailable` puts the version, which the dialog sets apart.
const VERSION_SLOT: &str = "<0>{{version}}</0>";

#[derive(Debug, Default)]
pub enum UpdatePhase {
    #[default]
    Hidden,
    Available(Arc<dyn AppUpdate>),
    /// Until the installed version starts. `progress` is `None` until the first bytes arrive.
    Downloading { update: Arc<dyn AppUpdate>, progress: Option<DownloadProgress> },
    Failed,
}

/// `useAppUpdate`'s state.
#[derive(Debug, Default)]
pub struct AppUpdateState {
    pub phase: UpdatePhase,
    /// An update is installed: the app starts it as it exits.
    pub relaunch: bool,
    checked: bool,
}

impl AppUpdateState {
    /// Whether the dialog shows.
    pub fn is_open(&self) -> bool {
        !matches!(self.phase, UpdatePhase::Hidden)
    }
}

/// `getStartupUpdate`: one check per run, never in a development build or in a distribution that
/// is updated from elsewhere. A failed check is only logged.
pub fn check_at_startup(store: &mut Store, services: &mut Services, distribution: DistributionKind, development: bool) {
    if store.app_update.checked || development || distribution == DistributionKind::Portable {
        return;
    }
    store.app_update.checked = true;
    let updates = services.updates.clone();
    services.workers.spawn("update-check", move || {
        let checked = updates.check();
        Box::new(move |store, _| match checked {
            Ok(Some(update)) => store.app_update.phase = UpdatePhase::Available(update),
            Ok(None) => {}
            Err(error) => log::error!("Update check failed: {error}"),
        })
    });
}

/// "Later", "Close", Escape and the scrim. A running update cannot be dismissed. The session
/// checks once, so a dismissed update stays away until the next start.
pub fn dismiss(store: &mut Store) {
    if !matches!(store.app_update.phase, UpdatePhase::Downloading { .. }) {
        store.app_update.phase = UpdatePhase::Hidden;
    }
}

/// "Update now": downloads, installs, then closes the app for the relaunch.
pub fn install_update(store: &mut Store, services: &mut Services) {
    let UpdatePhase::Available(update) = &store.app_update.phase else { return };
    let update = update.clone();
    store.app_update.phase = UpdatePhase::Downloading { update: update.clone(), progress: None };
    let ui_post = services.ui_post.clone();
    services.workers.spawn("update-download", move || {
        let latest = Arc::new(Mutex::new(None));
        let downloaded = update.download(&|progress| report_progress(&latest, ui_post.as_ref(), progress));
        Box::new(move |store, services| match downloaded {
            Ok(package) => {
                // Windows installs by ending the process, which skips every drop.
                services.preferences.flush();
                services.workers.spawn("update-install", move || {
                    let installed = update.install(package);
                    Box::new(move |store, services| match installed {
                        Ok(()) => {
                            store.app_update.relaunch = true;
                            services.window.close();
                        }
                        Err(error) => fail(store, &error),
                    })
                });
            }
            Err(error) => fail(store, &error),
        })
    });
}

fn fail(store: &mut Store, error: &str) {
    log::error!("Update installation failed: {error}");
    store.app_update.phase = UpdatePhase::Failed;
}

/// Keeps only the newest progress: one action is posted when the slot goes from empty to full.
fn report_progress(latest: &Arc<Mutex<Option<DownloadProgress>>>, ui_post: &dyn UiPost, progress: DownloadProgress) {
    if latest.lock().expect("update progress poisoned").replace(progress).is_some() {
        return;
    }
    let latest = latest.clone();
    ui_post.post(Box::new(move |store, _| {
        let newest = latest.lock().expect("update progress poisoned").take();
        if let UpdatePhase::Downloading { progress, .. } = &mut store.app_update.phase {
            *progress = newest;
        }
    }));
}

/// `getUpdateProgressPercent`, for a download of a known length.
fn progress_percent(progress: DownloadProgress) -> Option<f64> {
    let length = progress.content_length.filter(|length| *length > 0)?;
    Some((progress.downloaded as f64 / length as f64 * 100.0).min(100.0))
}

/// What `UpdatePromptDialog` shows.
#[derive(Debug, PartialEq)]
pub enum UpdatePromptView {
    Hidden,
    /// The announcement's text before and after the version.
    Available { version: String, before: String, after: String },
    /// `progress` is the percentage and its label, `None` while the length is unknown.
    Downloading { version: String, progress: Option<(f64, String)> },
    Failed,
}

pub fn update_prompt_view(store: &Store) -> UpdatePromptView {
    match &store.app_update.phase {
        UpdatePhase::Hidden => UpdatePromptView::Hidden,
        UpdatePhase::Available(update) => {
            let (before, after) = announcement(store.i18n.language);
            UpdatePromptView::Available { version: update.version().into(), before: before.into(), after: after.into() }
        }
        UpdatePhase::Downloading { update, progress } => UpdatePromptView::Downloading {
            version: update.version().into(),
            progress: progress.and_then(progress_percent).map(|percent| (percent, format_update_progress(percent))),
        },
        UpdatePhase::Failed => UpdatePromptView::Failed,
    }
}

fn announcement(locale: Locale) -> (&'static str, &'static str) {
    let text = catalog().t(locale, "app-update.versionNowAvailable");
    let (before, after) = text.split_once(VERSION_SLOT).expect("the announcement has the version's slot");
    (before.trim_end(), after.trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::{fake_services, FakeUpdate, Fakes, WindowCall};
    use crate::store::Dispatcher;
    use std::sync::atomic::Ordering;

    fn started(update: FakeUpdate) -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(serde_json::json!({}));
        fakes.updates.offer(update);
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, services| check_at_startup(store, services, DistributionKind::Installed, false));
        settle(&dispatcher, &fakes, |store| store.app_update.is_open());
        (dispatcher, fakes)
    }

    fn settle(dispatcher: &Dispatcher, fakes: &Fakes, done: impl Fn(&Store) -> bool) {
        for _ in 0..400 {
            fakes.ui_post.drain(dispatcher);
            if dispatcher.with_store(&done) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the update did not reach the expected state");
    }

    fn progress(downloaded: u64, content_length: Option<u64>) -> DownloadProgress {
        DownloadProgress { downloaded, content_length }
    }

    #[test]
    fn development_and_portable_builds_never_check() {
        let (parts, fakes) = fake_services(serde_json::json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, services| {
            check_at_startup(store, services, DistributionKind::Installed, true);
            check_at_startup(store, services, DistributionKind::Portable, false);
            assert!(!store.app_update.checked);
        });
        assert_eq!(fakes.updates.checks.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn later_hides_the_update_for_the_session() {
        let (dispatcher, fakes) = started(FakeUpdate::new("9.8.7"));
        assert_eq!(
            dispatcher.with_store(update_prompt_view),
            UpdatePromptView::Available { version: "9.8.7".into(), before: "OVRLEY".into(), after: "is now available.".into() }
        );
        dispatcher.dispatch(|store, services| {
            dismiss(store);
            check_at_startup(store, services, DistributionKind::Installed, false);
        });
        assert!(!dispatcher.with_store(|store| store.app_update.is_open()));
        assert_eq!(fakes.updates.checks.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_update_flushes_the_preferences_then_installs_and_relaunches() {
        let (dispatcher, fakes) = started(FakeUpdate::new("9.8.7"));
        dispatcher.dispatch(install_update);
        settle(&dispatcher, &fakes, |store| store.app_update.relaunch);
        assert!(fakes.updates.flushed_before_install.load(Ordering::SeqCst));
        assert_eq!(*fakes.window.calls.borrow(), [WindowCall::Close]);
    }

    #[test]
    fn a_running_update_cannot_be_dismissed() {
        let (dispatcher, _fakes) = started(FakeUpdate::new("9.8.7"));
        dispatcher.dispatch(|store, services| {
            install_update(store, services);
            dismiss(store);
            assert!(matches!(store.app_update.phase, UpdatePhase::Downloading { .. }));
        });
    }

    #[test]
    fn a_failed_download_and_a_failed_install_leave_the_app_running() {
        let failures = [
            FakeUpdate { download_error: Some("offline".into()), ..FakeUpdate::new("9.8.7") },
            FakeUpdate { install_error: Some("read-only".into()), ..FakeUpdate::new("9.8.7") },
        ];
        for update in failures {
            let (dispatcher, fakes) = started(update);
            dispatcher.dispatch(install_update);
            settle(&dispatcher, &fakes, |store| matches!(store.app_update.phase, UpdatePhase::Failed));
            assert!(!dispatcher.with_store(|store| store.app_update.relaunch));
            assert!(fakes.window.calls.borrow().is_empty());
            dispatcher.dispatch(|store, _| dismiss(store));
            assert!(!dispatcher.with_store(|store| store.app_update.is_open()));
        }
    }

    #[test]
    fn progress_is_determinate_only_with_a_length() {
        let (dispatcher, fakes) = started(FakeUpdate::new("9.8.7"));
        let shown = |progress| {
            dispatcher.dispatch(move |store, _| {
                let update = Arc::new(FakeUpdate::new("9.8.7"));
                store.app_update.phase = UpdatePhase::Downloading { update, progress };
            });
            match dispatcher.with_store(update_prompt_view) {
                UpdatePromptView::Downloading { progress, .. } => progress,
                view => panic!("not downloading: {view:?}"),
            }
        };
        assert_eq!(shown(None), None);
        assert_eq!(shown(Some(progress(10, None))), None);
        assert_eq!(shown(Some(progress(10, Some(0)))), None);
        assert_eq!(shown(Some(progress(25, Some(200)))), Some((12.5, "13%".into())));
        assert_eq!(shown(Some(progress(300, Some(200)))), Some((100.0, "100%".into())));

        // The newest of the reports that arrive between two turns of the UI thread.
        let latest = Arc::new(Mutex::new(None));
        report_progress(&latest, &fakes.ui_post, progress(50, Some(200)));
        report_progress(&latest, &fakes.ui_post, progress(150, Some(200)));
        assert_eq!(fakes.ui_post.posted.lock().unwrap().len(), 1);
        fakes.ui_post.drain(&dispatcher);
        assert!(matches!(
            dispatcher.with_store(update_prompt_view),
            UpdatePromptView::Downloading { progress: Some((percent, _)), .. } if percent == 75.0
        ));
    }

    #[test]
    fn every_translation_has_the_versions_slot() {
        for (locale, _) in crate::i18n::LOCALES {
            announcement(locale);
        }
    }
}

//! Port of `lib/preferences-store.js`: the `ovrley-settings.json` preferences.
//!
//! The file, keys and values are those the Tauri releases wrote, so an update keeps them (plan D8):
//! - the whole object is read once at startup; a missing file is the documented first run, and a
//!   file that is not a JSON object fails loudly;
//! - every write rewrites the whole object, pretty-printed as the plugin's default serializer does,
//!   so keys the app does not read are kept;
//! - writes are debounced by 100 ms, as the plugin's `autoSave`, go to a temporary file followed by
//!   a rename, and are flushed when the store is dropped.

use serde_json::{Map, Value};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

/// The file name `tauri-plugin-store` uses for OVRLEY's preferences.
pub const PREFERENCES_FILE_NAME: &str = "ovrley-settings.json";

/// The plugin's `autoSave` debounce.
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(100);

/// The preferences, as one JSON object.
pub type Preferences = Map<String, Value>;

/// Where the preferences go after a change.
pub trait PreferencesSink {
    /// Takes the whole object after a change.
    fn save(&mut self, preferences: &Preferences);
    /// Writes out every pending change before returning.
    fn flush(&mut self);
}

/// The preferences held in memory, with every change handed to a sink.
pub struct PreferencesStore {
    values: Preferences,
    /// Boxed so tests can record saves; [`FileSink`] in the app.
    sink: Box<dyn PreferencesSink>,
}

impl std::fmt::Debug for PreferencesStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("PreferencesStore").field("values", &self.values).finish_non_exhaustive()
    }
}

impl PreferencesStore {
    /// Reads `ovrley-settings.json` from the app-data directory and writes changes back to it.
    pub fn open(app_data_dir: &Path) -> Result<Self, String> {
        let path = app_data_dir.join(PREFERENCES_FILE_NAME);
        let values = read_preferences_file(&path)?;
        Ok(Self::with_sink(values, Box::new(FileSink::new(path))))
    }

    pub fn with_sink(values: Preferences, sink: Box<dyn PreferencesSink>) -> Self {
        Self { values, sink }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
        self.sink.save(&self.values);
    }

    pub fn delete(&mut self, key: &str) {
        if self.values.remove(key).is_some() {
            self.sink.save(&self.values);
        }
    }

    /// `getOptionalPathPreference`: an absent (or `null`) value is `None`; a present value must be
    /// a trimmed, non-empty, absolute path.
    pub fn get_optional_path(&self, key: &str) -> Result<Option<PathBuf>, String> {
        match self.values.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value))
                if !value.trim().is_empty()
                    && value == value.trim()
                    && Path::new(value).is_absolute() =>
            {
                Ok(Some(PathBuf::from(value)))
            }
            Some(_) => Err(format!("Preference \"{key}\" must be an absolute path")),
        }
    }

    pub fn flush(&mut self) {
        self.sink.flush();
    }
}

impl Drop for PreferencesStore {
    fn drop(&mut self) {
        self.sink.flush();
    }
}

/// Reads the preferences file. A missing file is the first run.
pub fn read_preferences_file(path: &Path) -> Result<Preferences, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Preferences::new()),
        Err(error) => return Err(format!("Could not read {}: {error}", path.display())),
    };
    match serde_json::from_slice(&bytes) {
        Ok(Value::Object(values)) => Ok(values),
        Ok(_) => Err(format!("{} must hold a JSON object", path.display())),
        Err(error) => Err(format!("{} is not valid JSON: {error}", path.display())),
    }
}

/// Writes the whole object pretty-printed, through a temporary file and a rename.
pub fn write_preferences_file(path: &Path, preferences: &Preferences) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(preferences).map_err(std::io::Error::other)?;
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

enum WriterMessage {
    Save(Box<Preferences>),
    Flush(Sender<()>),
}

/// Applies `write` on a background thread to the newest object once no change has arrived for
/// `delay`. A flush writes what is pending at once and waits for it.
pub struct DebouncedWriter {
    sender: Option<Sender<WriterMessage>>,
    thread: Option<JoinHandle<()>>,
}

impl DebouncedWriter {
    pub fn new(delay: Duration, write: impl FnMut(&Preferences) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("preferences-writer".into())
            .spawn(move || run_debounced_writer(receiver, delay, write))
            .expect("spawn the preferences writer");
        Self {
            sender: Some(sender),
            thread: Some(thread),
        }
    }

    pub fn save(&self, preferences: &Preferences) {
        self.send(WriterMessage::Save(Box::new(preferences.clone())));
    }

    pub fn flush(&self) {
        let (done, wait) = mpsc::channel();
        self.send(WriterMessage::Flush(done));
        // The writer thread only ends when this writer is dropped.
        wait.recv().expect("the preferences writer answers a flush");
    }

    fn send(&self, message: WriterMessage) {
        self.sender
            .as_ref()
            .expect("the writer is open until dropped")
            .send(message)
            .expect("the preferences writer is running");
    }
}

impl Drop for DebouncedWriter {
    fn drop(&mut self) {
        // Closing the channel makes the thread write what is pending and stop.
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                log::error!("the preferences writer panicked");
            }
        }
    }
}

fn run_debounced_writer(
    receiver: Receiver<WriterMessage>,
    delay: Duration,
    mut write: impl FnMut(&Preferences),
) {
    let mut pending: Option<Box<Preferences>> = None;
    loop {
        let message = if pending.is_some() {
            match receiver.recv_timeout(delay) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => {
                    write(&pending.take().expect("pending checked above"));
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match receiver.recv() {
                Ok(message) => message,
                Err(_) => break,
            }
        };
        match message {
            WriterMessage::Save(preferences) => pending = Some(preferences),
            WriterMessage::Flush(done) => {
                if let Some(preferences) = pending.take() {
                    write(&preferences);
                }
                // The flushing side waits on the answer; it is gone only if it panicked.
                let _ = done.send(());
            }
        }
    }
    if let Some(preferences) = pending {
        write(&preferences);
    }
}

/// The settings file, written by a [`DebouncedWriter`]. Write failures are logged and the app goes
/// on, as the plugin's `autoSave` does.
pub struct FileSink {
    writer: DebouncedWriter,
}

impl FileSink {
    pub fn new(path: PathBuf) -> Self {
        Self {
            writer: DebouncedWriter::new(SAVE_DEBOUNCE, move |preferences| {
                if let Err(error) = write_preferences_file(&path, preferences) {
                    log::warn!("Could not write {}: {error}", path.display());
                }
            }),
        }
    }
}

impl PreferencesSink for FileSink {
    fn save(&mut self, preferences: &Preferences) {
        self.writer.save(preferences);
    }

    fn flush(&mut self) {
        self.writer.flush();
    }
}

/// Records every saved object, for tests.
#[derive(Clone, Default)]
pub struct MemorySink {
    pub saves: Rc<RefCell<Vec<Preferences>>>,
    pub flushes: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl PreferencesSink for MemorySink {
    fn save(&mut self, preferences: &Preferences) {
        self.saves.borrow_mut().push(preferences.clone());
    }

    fn flush(&mut self) {
        self.flushes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl PreferencesStore {
    /// An in-memory store whose saves are recorded in the returned sink.
    pub fn in_memory(values: Value) -> (Self, MemorySink) {
        let Value::Object(values) = values else {
            panic!("in-memory preferences must be a JSON object");
        };
        let sink = MemorySink::default();
        (Self::with_sink(values, Box::new(sink.clone())), sink)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    pub(crate) fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ovrley-app-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_is_an_empty_first_run() {
        let dir = temp_dir("missing");
        let store = PreferencesStore::open(&dir).unwrap();
        assert_eq!(store.get("lng"), None);
    }

    #[test]
    fn malformed_file_fails() {
        let dir = temp_dir("malformed");
        std::fs::write(dir.join(PREFERENCES_FILE_NAME), "[1, 2]").unwrap();
        assert!(PreferencesStore::open(&dir)
            .unwrap_err()
            .contains("must hold a JSON object"));
        std::fs::write(dir.join(PREFERENCES_FILE_NAME), "{").unwrap();
        assert!(PreferencesStore::open(&dir)
            .unwrap_err()
            .contains("is not valid JSON"));
    }

    #[test]
    fn writes_keep_other_keys_pretty_printed_and_atomically() {
        let dir = temp_dir("keeps");
        let path = dir.join(PREFERENCES_FILE_NAME);
        std::fs::write(
            &path,
            r#"{"unitSystem":"imperial","mapTileCacheLimitBytes":1024}"#,
        )
        .unwrap();
        {
            let mut store = PreferencesStore::open(&dir).unwrap();
            store.set("lng", json!("de"));
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({"unitSystem": "imperial", "mapTileCacheLimitBytes": 1024, "lng": "de"})
        );
        assert!(text.contains("{\n  \""), "pretty-printed: {text}");
        assert!(!dir.join(format!("{PREFERENCES_FILE_NAME}.tmp")).exists());
    }

    #[test]
    fn writes_are_debounced_and_flushed() {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let recorded = writes.clone();
        let writer = DebouncedWriter::new(Duration::from_millis(200), move |preferences| {
            recorded.lock().unwrap().push(preferences.get("n").cloned());
        });
        for n in 0..5 {
            let mut preferences = Preferences::new();
            preferences.insert("n".into(), json!(n));
            writer.save(&preferences);
        }
        assert!(
            writes.lock().unwrap().is_empty(),
            "nothing is written within the debounce"
        );
        writer.flush();
        assert_eq!(*writes.lock().unwrap(), vec![Some(json!(4))]);

        let mut preferences = Preferences::new();
        preferences.insert("n".into(), json!(5));
        writer.save(&preferences);
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(
            writes.lock().unwrap().len(),
            2,
            "written once the debounce has passed"
        );
        drop(writer);
        assert_eq!(writes.lock().unwrap().len(), 2, "nothing pending on drop");
    }

    #[test]
    fn optional_path_rules() {
        let absolute = if cfg!(windows) {
            "C:\\Videos"
        } else {
            "/videos"
        };
        let (store, _) = PreferencesStore::in_memory(json!({
            "absent-null": null,
            "good": absolute,
            "relative": "videos",
            "padded": format!(" {absolute}"),
            "empty": "  ",
            "number": 3,
        }));
        assert_eq!(store.get_optional_path("missing").unwrap(), None);
        assert_eq!(store.get_optional_path("absent-null").unwrap(), None);
        assert_eq!(
            store.get_optional_path("good").unwrap(),
            Some(PathBuf::from(absolute))
        );
        for key in ["relative", "padded", "empty", "number"] {
            assert_eq!(
                store.get_optional_path(key).unwrap_err(),
                format!("Preference \"{key}\" must be an absolute path")
            );
        }
    }

    #[test]
    fn delete_saves_only_when_something_was_removed() {
        let (mut store, sink) = PreferencesStore::in_memory(json!({"a": 1}));
        store.delete("b");
        assert!(sink.saves.borrow().is_empty());
        store.delete("a");
        assert_eq!(sink.saves.borrow().as_slice(), &[Preferences::new()]);
    }
}

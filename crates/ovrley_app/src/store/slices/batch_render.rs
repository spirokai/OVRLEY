//! Port of `store/slices/createBatchRenderSlice.js`: the folder separate renders are written to,
//! which projects save, and the retained native snapshot of the running or finished export.

use ovrley_core::render_jobs::contracts::{BatchProgressUpdate, BatchSnapshot};

#[derive(Clone, Debug, Default)]
pub struct BatchRenderSlice {
    /// `batchOutputFolder`.
    pub batch_output_folder: Option<String>,
    /// `batchSnapshot`: the accepted export's latest snapshot, kept until the dialog closes.
    pub batch_snapshot: Option<BatchSnapshot>,
}

impl BatchRenderSlice {
    /// `setBatchOutputFolder`: a non-empty path, or none.
    pub fn set_batch_output_folder(&mut self, folder: Option<String>) -> Result<(), String> {
        if folder.as_deref().is_some_and(|folder| folder.trim().is_empty()) {
            return Err("Batch folder must be a nonempty path or null".into());
        }
        self.batch_output_folder = folder;
        Ok(())
    }

    /// `acceptBatchSnapshot`: the submission's first snapshot.
    pub fn accept_batch_snapshot(&mut self, snapshot: BatchSnapshot) {
        self.batch_snapshot = Some(snapshot);
    }

    /// `applyBatchSnapshot`: only of the accepted batch, at a newer revision.
    pub fn apply_batch_snapshot(&mut self, snapshot: BatchSnapshot) {
        if self.batch_snapshot.as_ref().is_some_and(|current| current.batch_id == snapshot.batch_id && snapshot.revision > current.revision) {
            self.batch_snapshot = Some(snapshot);
        }
    }

    /// A frame tick belongs to one queue transition: it applies only on that snapshot's revision,
    /// and only at a newer revision.
    pub fn apply_batch_tick(&mut self, tick: BatchProgressUpdate) {
        let Some(snapshot) = self.batch_snapshot.as_mut() else { return };
        if snapshot.batch_id != tick.batch_id || snapshot.snapshot_revision != tick.snapshot_revision || tick.revision <= snapshot.revision {
            return;
        }
        snapshot.revision = tick.revision;
        snapshot.processed_frames = tick.processed_frames;
        snapshot.rendered_frames = tick.rendered_frames;
        snapshot.encoded_frames = tick.encoded_frames;
        snapshot.current_item_progress = tick.current_item_progress;
        snapshot.elapsed_seconds = tick.elapsed_seconds;
        snapshot.estimated_seconds_remaining = tick.estimated_seconds_remaining;
    }

    /// `clearBatchResults`.
    pub fn clear_batch_results(&mut self) {
        self.batch_snapshot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_a_nonempty_path_or_none() {
        let mut slice = BatchRenderSlice::default();
        assert!(slice.set_batch_output_folder(Some("  ".into())).is_err());
        slice.set_batch_output_folder(Some("/renders".into())).unwrap();
        slice.set_batch_output_folder(None).unwrap();
        assert_eq!(slice.batch_output_folder, None);
    }
}

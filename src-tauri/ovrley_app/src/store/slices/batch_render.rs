//! Port of `store/slices/createBatchRenderSlice.js`'s durable half: the folder separate renders
//! are written to, which projects save. The render dialog sets it in Phase 7.

/// `batchOutputFolder`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchRenderSlice {
    pub batch_output_folder: Option<String>,
}

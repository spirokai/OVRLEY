//! Sparse storage for selected CSV activity columns.

use csv::StringRecord;

use super::headers::HeaderLayout;

/// Sparse, row-oriented storage for the columns selected from a CSV header.
///
/// Unrecognized columns are not retained. Recognized columns preserve their
/// original indices so short rows can still be read as missing values and
/// timeline errors can refer to the source record number.
pub(super) struct CsvColumnData {
    /// Zero-based source record index for each retained data row.
    record_indices: Vec<usize>,
    /// Values for selected columns, indexed by their original CSV position.
    columns: Vec<Option<Vec<String>>>,
    /// Original CSV positions whose values are retained in `columns`.
    selected_indices: Vec<usize>,
}

impl CsvColumnData {
    /// Creates empty storage sized for the recognized columns in `header`.
    pub(super) fn new(header: &HeaderLayout) -> Self {
        let width = header
            .columns
            .iter()
            .map(|column| column.index + 1)
            .max()
            .unwrap_or_default();
        let mut columns = (0..width).map(|_| None).collect::<Vec<_>>();
        let mut selected_indices = header
            .columns
            .iter()
            .map(|column| column.index)
            .collect::<Vec<_>>();
        if let Some(index) = header.gps_update_index {
            if index >= columns.len() {
                columns.resize_with(index + 1, || None);
            }
            selected_indices.push(index);
        }
        selected_indices.sort_unstable();
        selected_indices.dedup();
        for index in &selected_indices {
            columns[*index] = Some(Vec::new());
        }
        Self {
            record_indices: Vec::new(),
            columns,
            selected_indices,
        }
    }

    /// Appends a source record, treating absent cells in a short row as empty.
    pub(super) fn push(&mut self, record_index: usize, record: &StringRecord) {
        self.record_indices.push(record_index);
        for index in &self.selected_indices {
            self.columns[*index]
                .as_mut()
                .expect("selected CSV column has storage")
                .push(record.get(*index).unwrap_or_default().to_string());
        }
    }

    /// Returns the number of retained data rows.
    pub(super) fn len(&self) -> usize {
        self.record_indices.len()
    }

    /// Returns the original zero-based record index for a retained row.
    pub(super) fn record_index(&self, row: usize) -> usize {
        self.record_indices[row]
    }

    /// Returns a retained cell by retained row and original CSV column index.
    ///
    /// `None` means that the column was not selected, the row is short, or the
    /// requested row is outside the retained data range.
    pub(super) fn value(&self, row: usize, column: usize) -> Option<&str> {
        self.columns
            .get(column)?
            .as_ref()?
            .get(row)
            .map(String::as_str)
    }
}

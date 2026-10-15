//! An immutable editor snapshot links preparation, breakpoint acknowledgement, and startup execution.

use crate::{source_navigation, Document};
use std::path::{Path, PathBuf};

pub struct PreparedBreakpoint {
    pub token: u64,
    pub document_id: Option<usize>,
    pub original_path: PathBuf,
    pub line: usize,
    pub source_snapshot: Option<String>,
    pub run_after: bool,
    pub remaining_lines: Vec<usize>,
}

impl PreparedBreakpoint {
    pub fn current(&self, documents: &[Document], directory: &Path) -> bool {
        let Some(id) = self.document_id else {
            return true;
        };
        documents
            .iter()
            .find(|document| document.id == id)
            .is_some_and(|document| {
                self.source_snapshot
                    .as_ref()
                    .is_some_and(|source| &document.source == source)
                    && original_path(document, directory) == self.original_path
            })
    }
}

pub fn original_path(document: &Document, directory: &Path) -> PathBuf {
    source_navigation::identity(
        document
            .path
            .as_deref()
            .unwrap_or(&directory.join(&document.name)),
        directory,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_snapshot_rejects_edits_renames_and_closed_tabs_before_running() {
        let root = tempfile::tempdir().unwrap();
        let mut documents = vec![Document {
            id: 4,
            path: None,
            name: "untitled4.m".into(),
            source: "answer = later(3);".into(),
            saved_source: String::new(),
            cursor: 0,
            selection: None,
            executed_paths: Vec::new(),
        }];
        let pending = PreparedBreakpoint {
            token: 1,
            document_id: Some(4),
            original_path: original_path(&documents[0], root.path()),
            line: 3,
            source_snapshot: Some(documents[0].source.clone()),
            run_after: true,
            remaining_lines: Vec::new(),
        };
        assert!(pending.current(&documents, root.path()));
        documents[0].source.push_str("\nchanged = true;");
        assert!(!pending.current(&documents, root.path()));
        documents[0].source = pending.source_snapshot.clone().unwrap();
        documents[0].path = Some(root.path().join("renamed.m"));
        assert!(!pending.current(&documents, root.path()));
        assert!(!pending.current(&[], root.path()));
    }
}

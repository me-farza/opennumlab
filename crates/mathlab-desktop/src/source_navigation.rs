//! Source identity and line navigation work for saved files and unsaved editor buffers.

use crate::Document;
use mathlab_engine::StackFrame;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct SourceTarget {
    pub file: PathBuf,
    pub line: usize,
    pub function_name: String,
    pub document_id: Option<usize>,
}

pub fn identity(path: &Path, directory: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        directory.join(path)
    };
    path.canonicalize().unwrap_or_else(|_| {
        path.parent()
            .and_then(|parent| parent.canonicalize().ok())
            .zip(path.file_name())
            .map_or_else(|| path.clone(), |(parent, name)| parent.join(name))
    })
}

pub fn targets(
    frames: Vec<StackFrame>,
    documents: &[Document],
    directory: &Path,
    executed_document: Option<usize>,
    internal_worker: Option<&Path>,
) -> Vec<SourceTarget> {
    frames
        .into_iter()
        .filter_map(|frame| {
            if frame.file.is_empty() || frame.line == 0 {
                return None;
            }
            let file = identity(Path::new(&frame.file), directory);
            if internal_worker.is_some_and(|worker| identity(worker, directory) == file) {
                return None;
            }
            let matches = |document: &&Document| {
                document
                    .path
                    .as_ref()
                    .is_some_and(|path| identity(path, directory) == file)
                    || document
                        .executed_paths
                        .iter()
                        .any(|path| identity(path, directory) == file)
            };
            let document_id = documents
                .iter()
                .filter(|document| Some(document.id) == executed_document)
                .find(matches)
                .or_else(|| documents.iter().find(matches))
                .map(|document| document.id);
            Some(SourceTarget {
                file,
                line: frame.line,
                function_name: frame.name,
                document_id,
            })
        })
        .collect()
}

pub fn existing_document(
    documents: &[Document],
    target: &SourceTarget,
    directory: &Path,
) -> Option<usize> {
    target
        .document_id
        .and_then(|id| documents.iter().position(|document| document.id == id))
        .or_else(|| {
            documents.iter().position(|document| {
                document
                    .path
                    .as_ref()
                    .is_some_and(|path| identity(path, directory) == target.file)
                    || document
                        .executed_paths
                        .iter()
                        .any(|path| identity(path, directory) == target.file)
            })
        })
}

/// Byte span for the requested line, clipped to the current buffer without changing its source.
pub fn line_span(source: &str, line: usize) -> (usize, usize) {
    let total = source
        .chars()
        .filter(|&character| character == '\n')
        .count()
        + 1;
    let start = crate::search::line_start(source, line.clamp(1, total));
    let end = source[start..]
        .find('\n')
        .map_or(source.len(), |length| start + length);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(id: usize, path: Option<PathBuf>, source: &str) -> Document {
        Document {
            id,
            path,
            name: "untitled.m".into(),
            source: source.into(),
            saved_source: "old on-disk source".into(),
            cursor: 0,
            selection: None,
            executed_paths: Vec::new(),
        }
    }

    #[test]
    fn mapped_unsaved_error_resolves_its_existing_buffer_when_original_path_does_not_exist() {
        let root = tempfile::tempdir().unwrap();
        let source =
            "%% α\nanswer = later(3);\nfunction y = later(x)\n  missing_function();\nend\n";
        let path = identity(&root.path().join("untitled7.m"), root.path());
        let mut buffer = document(7, None, source);
        buffer.executed_paths.push(path.clone());
        let documents = vec![document(6, None, "other buffer"), buffer];
        let links = targets(
            vec![StackFrame {
                file: path.display().to_string(),
                name: "untitled7>later".into(),
                line: 4,
            }],
            &documents,
            root.path(),
            Some(7),
            None,
        );
        assert_eq!(links[0].document_id, Some(7));
        assert_eq!(
            existing_document(&documents, &links[0], root.path()),
            Some(1)
        );
        let (start, end) = line_span(&documents[1].source, links[0].line);
        assert_eq!(&documents[1].source[start..end], "  missing_function();");
        assert!(documents[1].dirty());
        assert_eq!(documents[1].source, source);
        assert!(!path.exists());
    }

    #[test]
    fn navigation_preserves_a_dirty_saved_tab_and_clips_stale_line_numbers() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("proof.m");
        std::fs::write(&path, "original disk content").unwrap();
        let documents = vec![document(9, Some(path.clone()), "new α\nunsaved change")];
        let links = targets(
            vec![StackFrame {
                file: path.display().to_string(),
                name: "proof".into(),
                line: 999,
            }],
            &documents,
            root.path(),
            None,
            None,
        );
        assert_eq!(
            existing_document(&documents, &links[0], root.path()),
            Some(0)
        );
        let (start, end) = line_span(&documents[0].source, 999);
        assert_eq!(&documents[0].source[start..end], "unsaved change");
        assert_eq!(documents[0].source, "new α\nunsaved change");
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "original disk content"
        );
    }

    #[test]
    fn only_exact_internal_worker_file_is_filtered() {
        let root = tempfile::tempdir().unwrap();
        let internal = root.path().join("mathlab_worker.m");
        let user = root.path().join("user/mathlab_worker.m");
        std::fs::create_dir_all(user.parent().unwrap()).unwrap();
        std::fs::write(&internal, "worker").unwrap();
        std::fs::write(&user, "user").unwrap();
        let frames = [&internal, &user]
            .into_iter()
            .map(|file| StackFrame {
                file: file.display().to_string(),
                name: "mathlab_worker".into(),
                line: 2,
            })
            .collect();
        let links = targets(frames, &[], root.path(), None, Some(&internal));
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].file, user.canonicalize().unwrap());
    }

    #[test]
    fn execution_owner_takes_precedence_over_another_tab_with_the_same_source_path() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("untitled7.m");
        std::fs::write(&path, "saved file").unwrap();
        let saved = document(2, Some(path.clone()), "saved file");
        let mut unsaved = document(7, None, "different running buffer");
        unsaved.executed_paths.push(path.clone());
        let documents = vec![saved, unsaved];
        let links = targets(
            vec![StackFrame {
                file: path.display().to_string(),
                name: "untitled7".into(),
                line: 1,
            }],
            &documents,
            root.path(),
            Some(7),
            None,
        );
        assert_eq!(links[0].document_id, Some(7));
        assert_eq!(
            existing_document(&documents, &links[0], root.path()),
            Some(1)
        );
        assert_eq!(documents[1].source, "different running buffer");
    }
}

/// Native locations refer to immutable captured bytes, never a fresh filesystem read.
pub(crate) fn native_target(sources:&[crate::rust_worker::SourceSnapshot],source_id:u64,line:usize)->Option<SourceTarget>{
    sources.iter().find(|s|s.source_id==source_id).map(|s|SourceTarget{file:s.path.clone(),line:line.max(1),function_name:String::new(),document_id:s.document_id})
}

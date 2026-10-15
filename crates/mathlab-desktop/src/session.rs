//! Project-local session recovery. Atomic replacement keeps an interrupted write recoverable.

use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct SavedDocument {
    pub path: Option<PathBuf>,
    pub name: String,
    pub source: String,
    pub saved_source: String,
    pub cursor: usize,
}

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub directory: PathBuf,
    pub executable: Option<PathBuf>,
    pub documents: Vec<SavedDocument>,
    pub active_document: usize,
    pub history: Vec<String>,
    pub show_folder: bool,
    pub show_workspace: bool,
    pub show_history: bool,
    pub ribbon_tab: String,
    pub figure_zoom: f32,
    #[serde(default)]
    pub debugger_bridge: Option<PathBuf>,
}

pub fn load(path: &Path) -> Result<Option<Session>, String> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > 16 * 1024 * 1024 => {
            return Err("Session recovery file exceeds the 16 MiB limit.".into())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        _ => {}
    }
    let data = fs::read(path).map_err(|error| error.to_string())?;
    let session: Session = serde_json::from_slice(&data).map_err(|error| error.to_string())?;
    if session.version != 1 {
        return Err(format!(
            "Session version {} is unsupported.",
            session.version
        ));
    }
    Ok(Some(session))
}

pub fn save(path: &Path, data: &[u8]) -> io::Result<()> {
    if data.len() > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Session recovery exceeds the 16 MiB limit. Save large buffers as scripts.",
        ));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("{}.pending", std::process::id()));
    {
        use io::Write;
        let mut file = fs::File::create(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
    }
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_preserves_unsaved_source_history_and_layout() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join(".mathlab/session.json");
        let session = Session {
            version: 1,
            directory: temporary.path().into(),
            executable: None,
            documents: vec![SavedDocument {
                path: None,
                name: "untitled1.m".into(),
                source: "A = 42;".into(),
                saved_source: String::new(),
                cursor: 4,
            }],
            active_document: 0,
            history: vec!["A = 42".into()],
            show_folder: false,
            show_workspace: true,
            show_history: false,
            ribbon_tab: "Editor".into(),
            figure_zoom: 1.5,
            debugger_bridge: None,
        };
        save(&path, &serde_json::to_vec(&session).unwrap()).unwrap();
        let restored = load(&path).unwrap().unwrap();
        assert_eq!(restored.documents[0].source, "A = 42;");
        assert!(restored.documents[0].saved_source.is_empty());
        assert_eq!(restored.history, ["A = 42"]);
        assert!(!restored.show_folder);
        assert_eq!(restored.figure_zoom, 1.5);
    }

    #[test]
    fn corrupt_or_future_session_is_reported_without_touching_source() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("session.json");
        fs::write(&path, "invalid json").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "invalid json");
    }
}

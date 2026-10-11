//! Shared storage names and native file replacement.
//!
//! PRG identifiers and the legacy state directory remain stable for existing
//! documents and installations; these constants do not introduce a new format.
use crate::Result;
use std::{
    ffi::OsStr,
    fs::File,
    path::{Path, PathBuf},
};

pub const PRG_EXTENSION: &str = "prg";
pub const PRG_MIME: &str = "application/vnd.project-graph";
pub const STAGE_ENTRY: &str = "stage.msgpack";
pub const METADATA_ENTRY: &str = "metadata.msgpack";
pub const TAGS_ENTRY: &str = "tags.msgpack";
pub const REFERENCE_ENTRY: &str = "reference.msgpack";
pub const ATTACHMENTS_PREFIX: &str = "attachments/";
pub const LEGACY_DOCUMENT_VERSION: &str = "2.3.0";
pub const RECOVERY_DOCUMENT_NAME: &str = "document.mio";
pub const LEGACY_RECOVERY_DOCUMENT_NAME: &str = "document.prg";
pub const RECOVERY_ENTRY_NAME: &str = "entry.json";
pub const RECOVERY_SETTINGS_NAME: &str = "settings.json";
pub const RECENT_FILE_NAME: &str = "recent.json";
pub const WORKFLOWS_FILE_NAME: &str = "workflows.json";
pub const COLLABORATION_DIR_NAME: &str = "code-collaboration";
const LEGACY_STATE_DIR_NAME: &str = "graplu";
const RECOVERY_DIR_NAME: &str = "recovery";

/// Preserve the state location used by existing Bevy installations.
pub fn state_root() -> Option<PathBuf> {
    state_root_from(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

fn state_root_from(xdg_state_home: Option<&OsStr>, user_home: Option<&OsStr>) -> Option<PathBuf> {
    xdg_state_home
        .map(PathBuf::from)
        .or_else(|| user_home.map(|home| PathBuf::from(home).join(".local/state")))
        .map(|root| root.join(LEGACY_STATE_DIR_NAME))
}

pub fn recent_path() -> Option<PathBuf> {
    state_root().map(|root| root.join(RECENT_FILE_NAME))
}

pub fn recovery_root() -> Option<PathBuf> {
    state_root().map(|root| root.join(RECOVERY_DIR_NAME))
}

/// Write and sync a sibling temporary file before replacing the destination.
/// The caller owns parent-directory creation and payload validation. A failed
/// write or replacement leaves the previous destination intact.
pub fn atomic_write(destination: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    write(temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    temporary.persist(destination)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};

    #[test]
    fn state_paths_keep_existing_xdg_and_home_locations() {
        let xdg = OsStr::new("/custom/state");
        let home = OsStr::new("/users/example");
        assert_eq!(
            state_root_from(Some(xdg), Some(home)),
            Some(PathBuf::from("/custom/state/graplu"))
        );
        let root = state_root_from(None, Some(home)).unwrap();
        assert_eq!(root, PathBuf::from("/users/example/.local/state/graplu"));
        assert_eq!(
            root.join(RECENT_FILE_NAME),
            PathBuf::from("/users/example/.local/state/graplu/recent.json")
        );
        assert_eq!(
            root.join(RECOVERY_DIR_NAME).join(RECOVERY_DOCUMENT_NAME),
            PathBuf::from("/users/example/.local/state/graplu/recovery/document.mio")
        );
        assert!(state_root_from(None, None).is_none());
    }

    #[test]
    fn failed_atomic_write_preserves_the_previous_record() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("record.json");
        fs::write(&destination, b"previous complete record").unwrap();
        let result = atomic_write(&destination, |file| {
            file.write_all(b"incomplete replacement")?;
            Err("payload validation failed".into())
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"previous complete record");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        atomic_write(&destination, |file| {
            file.write_all(b"next complete record")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"next complete record");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn atomic_write_does_not_create_missing_parents_or_replace_directories() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("missing");
        let destination = parent.join("record.json");
        assert!(atomic_write(&destination, |_| Ok(())).is_err());
        assert!(!parent.exists());

        let existing = directory.path().join("existing");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("keep.txt"), b"keep").unwrap();
        assert!(
            atomic_write(&existing, |file| {
                file.write_all(b"new record")?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(existing.join("keep.txt")).unwrap(), b"keep");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

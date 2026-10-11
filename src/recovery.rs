use crate::{Result, document::Document};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Entry {
    pub source: Option<PathBuf>,
    pub revision: u64,
    pub created_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub generations: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 60,
            generations: 10,
        }
    }
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        if !(15..=3600).contains(&self.interval_seconds) || !(1..=100).contains(&self.generations) {
            return Err("备份间隔须为 15–3600 秒，保留数量须为 1–100 份".into());
        }
        Ok(())
    }

    pub fn load(directory: &Path) -> Result<Self> {
        let file = match fs::File::open(directory.join("settings.json")) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        let policy: Self = serde_json::from_reader(file)?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn save(&self, directory: &Path) -> Result<()> {
        self.validate()?;
        fs::create_dir_all(directory)?;
        let file = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer(file.as_file(), self)?;
        file.as_file().sync_all()?;
        file.persist(directory.join("settings.json"))?;
        Ok(())
    }
}

#[derive(Default)]
pub struct Catalog {
    pub entries: Vec<(PathBuf, Entry)>,
    pub errors: Vec<String>,
}

pub fn discover(root: &Path) -> Result<Catalog> {
    let sessions = match fs::read_dir(root) {
        Ok(sessions) => sessions,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Catalog::default()),
        Err(error) => return Err(error.into()),
    };
    let mut catalog = Catalog::default();
    for session in sessions {
        let session = session?;
        if !session.file_type()?.is_dir()
            || uuid::Uuid::parse_str(&session.file_name().to_string_lossy()).is_err()
        {
            continue;
        }
        for tab in fs::read_dir(session.path())? {
            let tab = tab?;
            if !tab.file_type()?.is_dir()
                || tab.file_name().to_string_lossy().parse::<u64>().is_err()
            {
                continue;
            }
            for generation in fs::read_dir(tab.path())? {
                let generation = generation?;
                if !generation.file_type()?.is_dir()
                    || uuid::Uuid::parse_str(&generation.file_name().to_string_lossy()).is_err()
                {
                    continue;
                }
                let read = || -> Result<Entry> {
                    let metadata = serde_json::from_reader(fs::File::open(
                        generation.path().join("entry.json"),
                    )?)?;
                    if !generation.path().join("document.prg").is_file() {
                        return Err("缺少恢复文档".into());
                    }
                    Ok(metadata)
                };
                match read() {
                    Ok(entry) => catalog.entries.push((generation.path(), entry)),
                    Err(error) => catalog
                        .errors
                        .push(format!("{}：{error}", generation.path().display())),
                }
            }
        }
    }
    catalog
        .entries
        .sort_by_key(|(_, entry)| std::cmp::Reverse(entry.created_ms));
    Ok(catalog)
}

pub fn root() -> Result<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .map(|p| p.join("graplu/recovery"))
        .ok_or_else(|| "无法确定恢复副本目录".into())
}

pub fn remove(root: &Path, generation: &Path) -> Result<()> {
    let relative = generation
        .strip_prefix(root)
        .map_err(|_| "副本不在恢复目录中")?;
    let components: Vec<_> = relative.components().collect();
    if components.len() != 3
        || components
            .iter()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("恢复副本路径无效".into());
    }
    if uuid::Uuid::parse_str(&components[0].as_os_str().to_string_lossy()).is_err()
        || components[1]
            .as_os_str()
            .to_string_lossy()
            .parse::<u64>()
            .is_err()
        || uuid::Uuid::parse_str(&components[2].as_os_str().to_string_lossy()).is_err()
    {
        return Err("恢复副本路径无效".into());
    }
    let mut checked = fs::canonicalize(root)?;
    for component in components {
        checked.push(component);
        let metadata = match fs::symlink_metadata(&checked) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("恢复副本目录不能是符号链接或普通文件".into());
        }
    }
    // A recovery generation contains only these two files, never user folders.
    for item in fs::read_dir(&checked)? {
        let item = item?;
        if !matches!(
            item.file_name().to_str(),
            Some("document.prg" | "entry.json")
        ) || !item.file_type()?.is_file()
        {
            return Err("副本目录包含额外内容，未执行删除".into());
        }
    }
    fs::remove_dir_all(checked)?;
    Ok(())
}

/// A generation becomes discoverable only after both its PRG and metadata exist.
pub fn write(document: &Document, directory: &Path) -> Result<PathBuf> {
    write_with_policy(document, directory, &Policy::default())
}

pub fn write_with_policy(
    document: &Document,
    directory: &Path,
    policy: &Policy,
) -> Result<PathBuf> {
    policy.validate()?;
    fs::create_dir_all(directory)?;
    let pending = tempfile::tempdir_in(directory)?;
    document.save_snapshot(&pending.path().join("document.prg"))?;
    let metadata = Entry {
        source: document.path.clone(),
        revision: document.revision,
        created_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    };
    let file = fs::File::create(pending.path().join("entry.json"))?;
    serde_json::to_writer(&file, &metadata)?;
    file.sync_all()?;
    let destination = directory.join(uuid::Uuid::new_v4().to_string());
    fs::rename(pending.path(), &destination)?;
    for (path, _) in list(directory)?
        .into_iter()
        .filter(|(path, _)| path != &destination)
        .skip(policy.generations - 1)
    {
        fs::remove_dir_all(path)?;
    }
    Ok(destination)
}

pub fn list(directory: &Path) -> Result<Vec<(PathBuf, Entry)>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let mut result = Vec::new();
    for item in entries {
        let item = item?;
        if uuid::Uuid::parse_str(&item.file_name().to_string_lossy()).is_err()
            || !item.file_type()?.is_dir()
        {
            continue;
        }
        let metadata = serde_json::from_reader(fs::File::open(item.path().join("entry.json"))?)?;
        result.push((item.path(), metadata));
    }
    result.sort_by_key(|(_, entry): &(PathBuf, Entry)| std::cmp::Reverse(entry.created_ms));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_one_generation_keeps_other_sessions_and_rejects_unrelated_paths() {
        let root = tempfile::tempdir().unwrap();
        let mut document = Document::blank().unwrap();
        document.create_text_node([0.0; 2]).unwrap();
        let first = write(
            &document,
            &root.path().join(uuid::Uuid::new_v4().to_string()).join("1"),
        )
        .unwrap();
        let second = write(
            &document,
            &root.path().join(uuid::Uuid::new_v4().to_string()).join("1"),
        )
        .unwrap();
        let bytes = fs::read(second.join("document.prg")).unwrap();
        assert!(remove(root.path(), root.path()).is_err());
        assert!(remove(root.path(), first.parent().unwrap()).is_err());
        assert!(remove(root.path(), &first.join("../elsewhere")).is_err());
        fs::write(first.join("user.txt"), b"keep").unwrap();
        assert!(remove(root.path(), &first).is_err());
        assert_eq!(fs::read(first.join("user.txt")).unwrap(), b"keep");
        fs::remove_file(first.join("user.txt")).unwrap();
        remove(root.path(), &first).unwrap();
        remove(root.path(), &first).unwrap();
        assert!(!first.exists());
        assert_eq!(fs::read(second.join("document.prg")).unwrap(), bytes);
        assert_eq!(discover(root.path()).unwrap().entries.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn deletion_rejects_symlinked_sessions() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let generation = write(&doc, &outside.path().join("1")).unwrap();
        let session = root.path().join(uuid::Uuid::new_v4().to_string());
        std::os::unix::fs::symlink(outside.path(), &session).unwrap();
        let alias = session.join("1").join(generation.file_name().unwrap());
        assert!(remove(root.path(), &alias).is_err());
        assert!(generation.join("document.prg").exists());
    }

    #[test]
    fn policy_roundtrip_validates_values_and_controls_retention() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(Policy::load(root.path()).unwrap(), Policy::default());
        let mut policy = Policy {
            enabled: false,
            interval_seconds: 120,
            generations: 2,
        };
        policy.save(root.path()).unwrap();
        assert_eq!(Policy::load(root.path()).unwrap(), policy);
        let previous = fs::read(root.path().join("settings.json")).unwrap();
        policy.generations = 0;
        assert!(policy.save(root.path()).is_err());
        assert_eq!(
            fs::read(root.path().join("settings.json")).unwrap(),
            previous
        );
        policy.generations = 2;
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let directory = root.path().join("generations");
        for _ in 0..5 {
            write_with_policy(&doc, &directory, &policy).unwrap();
        }
        assert_eq!(list(&directory).unwrap().len(), 2);
        fs::write(root.path().join("settings.json"), b"invalid").unwrap();
        assert!(Policy::load(root.path()).is_err());
    }

    #[test]
    fn catalog_keeps_valid_generations_and_recovery_opens_as_unsaved_draft() {
        let root = tempfile::tempdir().unwrap();
        let tab = root.path().join(uuid::Uuid::new_v4().to_string()).join("1");
        let mut document = Document::blank().unwrap();
        document.create_text_node([0.0; 2]).unwrap();
        let generation = write(&document, &tab).unwrap();
        let corrupt = tab.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&corrupt).unwrap();
        fs::write(corrupt.join("entry.json"), b"bad json").unwrap();
        let catalog = discover(root.path()).unwrap();
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.errors.len(), 1);
        let source = generation.join("document.prg");
        let bytes = fs::read(&source).unwrap();
        let mut restored = Document::open_recovery(&source).unwrap();
        assert!(restored.path.is_none());
        assert!(restored.dirty());
        restored.create_text_node([100.0; 2]).unwrap();
        restored.undo();
        assert!(restored.dirty());
        restored.save(&root.path().join("restored.prg")).unwrap();
        assert!(!restored.dirty());
        assert_eq!(restored.nodes.len(), 1);
        assert_eq!(fs::read(source).unwrap(), bytes);
    }

    #[test]
    fn captured_generation_is_independent_and_incomplete_writes_are_not_listed() {
        let directory = tempfile::tempdir().unwrap();
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let snapshot = doc.capture_snapshot().unwrap();
        doc.create_text_node([500.0; 2]).unwrap();
        let path = write(&snapshot, directory.path()).unwrap();
        assert_eq!(
            Document::open(&path.join("document.prg"))
                .unwrap()
                .nodes
                .len(),
            1
        );
        assert_eq!(doc.nodes.len(), 2);
        assert!(doc.dirty());
        fs::create_dir(directory.path().join(".tmp-incomplete")).unwrap();
        let entries = list(directory.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.revision, snapshot.revision);
        assert!(entries[0].1.source.is_none());
        for _ in 0..12 {
            write(&snapshot, directory.path()).unwrap();
        }
        assert_eq!(list(directory.path()).unwrap().len(), 10);
    }
}

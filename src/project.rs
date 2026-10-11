//! Ravomi's portable project container. The canvas codec is independent of the
//! container version; PRG is a compatibility format, never a renamed MIO file.
use crate::{Result, project_history::History};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Seek},
    path::{Path, PathBuf},
};

pub const EXTENSION: &str = "mio";
pub const DEFAULT_FILE_NAME: &str = "未命名项目.mio";
pub const FILTER_NAME: &str = "Ravomi 项目 (.mio)";
pub use crate::storage::PRG_EXTENSION as LEGACY_EXTENSION;
pub const MIME: &str = "application/x-ravomi";
pub const MANIFEST_ENTRY: &str = "manifest.json";
pub use crate::project_history::HISTORY_ENTRY;
pub const LEGACY_PREFIX: &str = "extensions/legacy/";
const FORMAT_ID: &str = "ravomi-project";
pub const SCHEMA_VERSION: u32 = 3;
pub const SQLITE_SCHEMA_VERSION: u32 = SCHEMA_VERSION;
pub const ZIP_SCHEMA_VERSION: u32 = 2;
pub const SQLITE_FEATURE: &str = "sqlite-graph-v1";
pub const SNAPSHOT_FEATURE: &str = "portable-snapshot-v1";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_HISTORY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PROJECT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub schema_version: u32,
    pub project_id: String,
    pub canvas_schema_version: u32,
    #[serde(default)]
    pub required_features: Vec<String>,
    /// Optional producer metadata survives an edit in an older application.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            format: FORMAT_ID.into(),
            schema_version: SCHEMA_VERSION,
            project_id: uuid::Uuid::new_v4().to_string(),
            canvas_schema_version: 1,
            required_features: vec![SQLITE_FEATURE.into()],
            extensions: BTreeMap::new(),
        }
    }
}

impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if self.format != FORMAT_ID
            || !matches!(
                self.schema_version,
                1 | ZIP_SCHEMA_VERSION | SQLITE_SCHEMA_VERSION
            )
            || self.canvas_schema_version != 1
        {
            return Err("不支持的 Ravomi 项目格式或版本，请使用兼容的应用打开".into());
        }
        if uuid::Uuid::parse_str(&self.project_id).is_err() {
            return Err("项目身份无效".into());
        }
        let expected = if self.schema_version == SQLITE_SCHEMA_VERSION {
            vec![SQLITE_FEATURE.to_owned()]
        } else if self.schema_version == ZIP_SCHEMA_VERSION {
            vec![SNAPSHOT_FEATURE.to_owned()]
        } else {
            vec![]
        };
        if self.required_features != expected {
            return Err(format!(
                "项目需要尚未支持的能力：{}",
                self.required_features.join("、")
            )
            .into());
        }
        Ok(())
    }

    pub(crate) fn for_save(&self) -> Self {
        let mut manifest = self.clone();
        manifest.schema_version = ZIP_SCHEMA_VERSION;
        manifest.required_features = vec![SNAPSHOT_FEATURE.into()];
        manifest
    }

    pub(crate) fn for_database(&self) -> Self {
        let mut manifest = self.clone();
        manifest.schema_version = SQLITE_SCHEMA_VERSION;
        manifest.required_features = vec![SQLITE_FEATURE.into()];
        manifest
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Mio,
    Prg,
}

impl Format {
    pub fn stage(self) -> &'static str {
        match self {
            Self::Mio => "canvas/stage.msgpack",
            Self::Prg => crate::storage::STAGE_ENTRY,
        }
    }

    pub fn assets(self) -> &'static str {
        match self {
            Self::Mio => "assets/",
            Self::Prg => crate::storage::ATTACHMENTS_PREFIX,
        }
    }

    pub fn for_destination(path: &Path) -> Result<Self> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case(EXTENSION) => Ok(Self::Mio),
            Some(extension) if extension.eq_ignore_ascii_case(LEGACY_EXTENSION) => Ok(Self::Prg),
            _ => Err("项目文件请使用 .mio；兼容画布导出请使用 .prg".into()),
        }
    }
}

/// Used before both tab-conflict checks and writes, so an omitted extension
/// cannot bypass the check for a file already open in another tab.
pub fn save_path(path: &Path) -> Result<PathBuf> {
    if path.file_name().is_none() {
        return Err("保存路径缺少文件名".into());
    }
    let mut destination = path.to_path_buf();
    if path.extension().is_none() {
        destination.set_extension(EXTENSION);
    }
    Format::for_destination(&destination)?;
    Ok(destination)
}

pub(crate) struct Contents {
    pub format: Format,
    pub manifest: Manifest,
    pub history: History,
    pub bindings: Option<Vec<u8>>,
}

pub(crate) fn inspect<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Contents> {
    let has_manifest = archive.file_names().any(|name| name == MANIFEST_ENTRY);
    let has_legacy_stage = archive
        .file_names()
        .any(|name| name == crate::storage::STAGE_ENTRY);
    if has_legacy_stage {
        // PRG has always allowed opaque extra entries. A third-party manifest
        // or canvas/notes entry must not turn an existing PRG into a MIO file.
        if has_manifest && archive.by_name(MANIFEST_ENTRY)?.size() <= MAX_MANIFEST_BYTES {
            let bytes = read_entry(archive, MANIFEST_ENTRY, MAX_MANIFEST_BYTES)?;
            if serde_json::from_slice::<serde_json::Value>(&bytes)
                .is_ok_and(|value| value["format"] == FORMAT_ID)
            {
                return Err("项目同时包含旧画布入口与新格式标识".into());
            }
        }
        return Ok(Contents {
            format: Format::Prg,
            manifest: Manifest::default(),
            history: History::default(),
            bindings: None,
        });
    }
    if !has_manifest {
        return Err("项目缺少 manifest.json 或兼容画布 stage.msgpack".into());
    }
    validate_entries(archive)?;
    let manifest: Manifest =
        serde_json::from_slice(&read_entry(archive, MANIFEST_ENTRY, MAX_MANIFEST_BYTES)?)?;
    manifest.validate()?;
    if manifest.schema_version == SQLITE_SCHEMA_VERSION {
        return Err("SQLite 项目不能使用 ZIP 容器".into());
    }
    let (history, bindings) = if manifest.schema_version == ZIP_SCHEMA_VERSION {
        crate::project_package::read(archive, &manifest)?
    } else {
        (
            serde_json::from_slice(&read_entry(archive, HISTORY_ENTRY, MAX_HISTORY_BYTES)?)?,
            None,
        )
    };
    history.validate()?;
    Ok(Contents {
        format: Format::Mio,
        manifest,
        history,
        bindings,
    })
}

pub(crate) fn read_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>> {
    let entry = archive.by_name(name)?;
    if entry.size() > limit {
        return Err(format!("项目条目 {name} 超出大小限制").into());
    }
    let mut bytes = Vec::new();
    entry.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(format!("项目条目 {name} 超出大小限制").into());
    }
    Ok(bytes)
}

pub(crate) fn validate_entry_name(name: &str) -> Result<()> {
    let path = name.trim_end_matches('/');
    if path.is_empty()
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("项目条目路径无效：{name}").into());
    }
    Ok(())
}

fn validate_entries<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Result<()> {
    if archive.len() > 16_384 {
        return Err("项目条目过多".into());
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        validate_entry_name(entry.name())?;
        if !names.insert(entry.name().to_owned()) {
            return Err(format!("重复项目条目：{}", entry.name()).into());
        }
        total = total.checked_add(entry.size()).ok_or("项目大小溢出")?;
        if total > MAX_PROJECT_BYTES {
            return Err("项目解压后超过 512 MiB".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_names_have_one_explicit_container_format() {
        assert_eq!(
            save_path(Path::new("中文项目")).unwrap(),
            Path::new("中文项目.mio")
        );
        assert_eq!(
            save_path(Path::new("old.PRG")).unwrap(),
            Path::new("old.PRG")
        );
        assert_eq!(
            save_path(Path::new("new.MIO")).unwrap(),
            Path::new("new.MIO")
        );
        assert!(save_path(Path::new("image.png")).is_err());
        assert!(save_path(Path::new("/")).is_err());
    }

    #[test]
    fn manifest_rejects_future_required_schemas_and_preserves_optional_metadata() {
        let mut manifest = Manifest::default();
        manifest
            .extensions
            .insert("producer".into(), serde_json::json!({"future": 2}));
        let parsed: Manifest =
            serde_json::from_slice(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(parsed, manifest);
        parsed.validate().unwrap();
        manifest.schema_version += 1;
        assert!(manifest.validate().is_err());
        manifest.schema_version = 1;
        manifest.required_features.push("future-execution".into());
        assert!(manifest.validate().is_err());
    }
}

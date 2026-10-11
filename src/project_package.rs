//! Versioned portable evidence. This is a document capture, not a runtime
//! transaction watermark or permission to resume any archived job.
use crate::{
    Result,
    project::{self, Manifest},
    project_history::History,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek},
};

pub(crate) const SNAPSHOT_ENTRY: &str = "collaboration/snapshot.json";
pub(crate) const RECORDS_ENTRY: &str = "collaboration/records/history.msgpack";
pub(crate) const BINDINGS_ENTRY: &str = "canvas/bindings.json";
pub(crate) const BINDINGS_ATTACHMENT: &str = "ravomi-code-agents.json";
const OBJECT_PREFIX: &str = "objects/sha256/";
const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObjectRef {
    sha256: String,
    size: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    schema_version: u32,
    snapshot_id: String,
    project_id: String,
    document_revision: u64,
    selected_workflow_ids: Vec<String>,
    entries: BTreeMap<String, ObjectRef>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bindings {
    schema_version: u32,
    /// Exact legacy bytes survive migration; checkpoint bindings will require
    /// their own declared feature, rather than inventing history for old nodes.
    legacy_workflow_bindings: Option<ObjectRef>,
}

fn digest(bytes: &[u8]) -> ObjectRef {
    ObjectRef {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size: bytes.len() as u64,
    }
}

fn validate_ref(reference: &ObjectRef) -> Result<()> {
    if reference.sha256.len() != 64
        || !reference
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || reference.size > MAX_BYTES
    {
        return Err("项目内容引用无效或超出大小限制".into());
    }
    Ok(())
}

/// Bound decompression by the declared ZIP size even while checking integrity.
fn checksum(reader: impl Read, declared_size: u64) -> Result<ObjectRef> {
    let mut hasher = Sha256::new();
    let limit = declared_size.checked_add(1).ok_or("项目条目大小溢出")?;
    let size = std::io::copy(&mut reader.take(limit), &mut hasher)?;
    if size != declared_size {
        return Err("项目条目声明大小与实际正文不一致".into());
    }
    Ok(ObjectRef {
        sha256: format!("{:x}", hasher.finalize()),
        size,
    })
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    manifest.validate()?;
    if manifest.schema_version != project::ZIP_SCHEMA_VERSION {
        return Err("内容寻址项目包需要当前容器版本".into());
    }
    Ok(())
}

pub(crate) fn managed(name: &str) -> bool {
    matches!(name, SNAPSHOT_ENTRY | RECORDS_ENTRY | BINDINGS_ENTRY)
        || name.starts_with(OBJECT_PREFIX)
}

pub(crate) fn prepare(
    manifest: &Manifest,
    history: &History,
    bindings: Option<&Vec<u8>>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    validate_manifest(manifest)?;
    history.validate()?;
    let mut entries = BTreeMap::new();
    let mut expanded = 0u64;
    let mut put = |bytes: Vec<u8>| -> Result<ObjectRef> {
        expanded = expanded
            .checked_add(bytes.len() as u64)
            .ok_or("项目正文大小溢出")?;
        if expanded > MAX_BYTES {
            return Err("项目协作正文展开后超过 64 MiB".into());
        }
        let reference = digest(&bytes);
        entries
            .entry(format!("{OBJECT_PREFIX}{}", reference.sha256))
            .or_insert(bytes);
        Ok(reference)
    };
    let records = history
        .clone()
        .try_map_text(|text| put(text.into_bytes()))?;
    let bindings = Bindings {
        schema_version: 1,
        legacy_workflow_bindings: bindings.map(|bytes| put(bytes.clone())).transpose()?,
    };
    entries.insert(
        project::MANIFEST_ENTRY.into(),
        serde_json::to_vec(manifest)?,
    );
    entries.insert(RECORDS_ENTRY.into(), rmp_serde::to_vec_named(&records)?);
    entries.insert(BINDINGS_ENTRY.into(), serde_json::to_vec(&bindings)?);
    Ok(entries)
}

/// Seal the finished temporary ZIP. Hash uncompressed bytes (including opaque
/// extensions) so changing compression never changes content identity.
pub(crate) fn seal(
    file: &mut std::fs::File,
    manifest: &Manifest,
    history: &History,
    revision: u64,
) -> Result<()> {
    validate_manifest(manifest)?;
    history.validate()?;
    let mut archive = zip::ZipArchive::new(&mut *file)?;
    let mut entries = BTreeMap::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_owned();
        project::validate_entry_name(&name)?;
        if name == SNAPSHOT_ENTRY || entries.contains_key(&name) {
            return Err("项目封存包含重复条目或已有保存快照".into());
        }
        let size = entry.size();
        entries.insert(name, checksum(entry, size)?);
    }
    drop(archive);
    let snapshot = Snapshot {
        schema_version: 1,
        snapshot_id: uuid::Uuid::new_v4().to_string(),
        project_id: manifest.project_id.clone(),
        document_revision: revision,
        selected_workflow_ids: history
            .workflows
            .iter()
            .map(|workflow| workflow.id.clone())
            .collect(),
        entries,
    };
    let mut writer = zip::ZipWriter::new_append(file)?;
    writer.start_file(
        SNAPSHOT_ENTRY,
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated),
    )?;
    serde_json::to_writer(&mut writer, &snapshot)?;
    writer.finish()?;
    Ok(())
}

pub(crate) fn read<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    manifest: &Manifest,
) -> Result<(History, Option<Vec<u8>>)> {
    validate_manifest(manifest)?;
    let snapshot: Snapshot = serde_json::from_slice(&project::read_entry(
        archive,
        SNAPSHOT_ENTRY,
        4 * 1024 * 1024,
    )?)?;
    if snapshot.schema_version != 1
        || uuid::Uuid::parse_str(&snapshot.snapshot_id).is_err()
        || snapshot.project_id != manifest.project_id
        || snapshot.entries.len() + 1 != archive.len()
        || snapshot.entries.contains_key(SNAPSHOT_ENTRY)
    {
        return Err("项目保存快照无效或条目不完整".into());
    }
    let mut objects = BTreeSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.name() == SNAPSHOT_ENTRY {
            continue;
        }
        let expected = snapshot
            .entries
            .get(entry.name())
            .ok_or("项目包含未封存条目")?;
        let name = entry.name().to_owned();
        let size = entry.size();
        let actual = checksum(entry, size)?;
        if expected != &actual {
            return Err(format!("项目内容校验失败：{name}").into());
        }
        if let Some(hash) = name.strip_prefix(OBJECT_PREFIX) {
            validate_ref(&actual)?;
            if hash != actual.sha256 {
                return Err(format!("内容对象名称与正文哈希不一致：{name}").into());
            }
            objects.insert(name);
        }
    }
    let records: History<ObjectRef> =
        rmp_serde::from_slice(&project::read_entry(archive, RECORDS_ENTRY, MAX_BYTES)?)?;
    records.validate()?;
    if snapshot.selected_workflow_ids
        != records
            .workflows
            .iter()
            .map(|workflow| workflow.id.clone())
            .collect::<Vec<_>>()
    {
        return Err("项目快照所选历史与记录不一致".into());
    }
    let bindings: Bindings =
        serde_json::from_slice(&project::read_entry(archive, BINDINGS_ENTRY, 64 * 1024)?)?;
    if bindings.schema_version != 1
        || archive.file_names().any(|name| {
            name == format!("assets/{BINDINGS_ATTACHMENT}") || name == project::HISTORY_ENTRY
        })
    {
        return Err("项目绑定版本无效或存在重复事实来源".into());
    }
    let mut expanded = 0u64;
    let mut referenced = BTreeSet::new();
    let mut cache = BTreeMap::<String, Vec<u8>>::new();
    let mut resolve = |reference: ObjectRef| -> Result<Vec<u8>> {
        validate_ref(&reference)?;
        expanded = expanded
            .checked_add(reference.size)
            .ok_or("项目正文大小溢出")?;
        if expanded > MAX_BYTES {
            return Err("项目协作正文展开后超过 64 MiB".into());
        }
        let name = format!("{OBJECT_PREFIX}{}", reference.sha256);
        // The sealing pass already verified every object's bytes and filename.
        // Verify each reference against that evidence, then decode an object
        // only once even if many historical inputs share its body.
        if snapshot.entries.get(&name) != Some(&reference) {
            return Err(format!("内容对象校验失败：{name}").into());
        }
        referenced.insert(name.clone());
        if let Some(bytes) = cache.get(&name) {
            return Ok(bytes.clone());
        }
        let bytes = project::read_entry(archive, &name, reference.size)?;
        cache.insert(name, bytes.clone());
        Ok(bytes)
    };
    let history = records.try_map_text(|reference| Ok(String::from_utf8(resolve(reference)?)?))?;
    let bindings = bindings.legacy_workflow_bindings.map(resolve).transpose()?;
    // This feature owns a closed object pool. Opaque extensions must store
    // their own bytes elsewhere until a feature declares additional references.
    if objects != referenced {
        return Err("项目包含未被协作历史或画布绑定引用的内容对象".into());
    }
    Ok((history, bindings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::Document,
        project_history::{
            ArchivedFile, ArchivedProposal, ArchivedRun, ArchivedStatus, ArchivedWorkflow,
        },
    };
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
    };

    const BODY: &str = "同一段保留的正文 / user-provided evidence";
    const BINDING_BYTES: &[u8] = br#"{"version":1,"nodes":{"node-a":{"workflowId":"workflow-0"}}}"#;

    fn history(body: &str, workflows: usize) -> History {
        History {
            schema_version: 1,
            workflows: (0..workflows)
                .map(|index| ArchivedWorkflow {
                    id: format!("workflow-{index}"),
                    goal: body.into(),
                    model: "test/model".into(),
                    files: vec![ArchivedFile {
                        path: "source.txt".into(),
                        content: body.into(),
                    }],
                    plan: None,
                    draft_plan: None,
                    runs: vec![ArchivedRun {
                        id: format!("run-{index}"),
                        task_id: "$plan".into(),
                        status: ArchivedStatus::Failed,
                        created_at: 10,
                        finished_at: Some(20),
                        sources: vec![],
                        prompt: body.into(),
                        input_files: vec![ArchivedFile {
                            path: "input.txt".into(),
                            content: body.into(),
                        }],
                        output: body.into(),
                        proposal: Some(ArchivedProposal {
                            summary: body.into(),
                            files: vec![ArchivedFile {
                                path: "proposal.txt".into(),
                                content: body.into(),
                            }],
                        }),
                        error: Some(body.into()),
                        review: Some("Historical review only".into()),
                        usage: Some(serde_json::json!({"tokens": 12})),
                        retry_of: None,
                    }],
                })
                .collect(),
        }
    }

    struct Fixture {
        directory: tempfile::TempDir,
        path: PathBuf,
        history: History,
    }

    fn fixture(history: History) -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.mio");
        let mut document = Document::blank().unwrap();
        document.attach_history(history.clone()).unwrap();
        document
            .attachments
            .insert(BINDINGS_ATTACHMENT.into(), BINDING_BYTES.to_vec());
        // Keep exercising the legacy v2 reader after native saves switch to SQLite.
        document.write_archive(&path, true, false).unwrap();
        Fixture {
            directory,
            path,
            history,
        }
    }

    fn entries(path: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        (0..archive.len())
            .map(|index| {
                let mut entry = archive.by_index(index).unwrap();
                let name = entry.name().to_owned();
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                (name, bytes)
            })
            .collect()
    }

    /// Rebuilding produces valid ZIP CRCs, so failures exercise the project
    /// checksums or semantic contract rather than ZIP's transport integrity.
    fn write_entries(
        path: &Path,
        mut entries: BTreeMap<String, Vec<u8>>,
        reseal: Option<&History>,
    ) {
        if reseal.is_some() {
            entries.remove(SNAPSHOT_ENTRY);
        }
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();
        let mut writer = zip::ZipWriter::new(&mut file);
        for (name, bytes) in &entries {
            writer
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
        if let Some(history) = reseal {
            let manifest = serde_json::from_slice(&entries[project::MANIFEST_ENTRY]).unwrap();
            seal(&mut file, &manifest, history, 0).unwrap();
        }
    }

    fn open_error(path: &Path) -> String {
        Document::open(path)
            .err()
            .expect("malformed package must be rejected")
            .to_string()
    }

    #[test]
    fn repeated_bodies_share_objects_and_roundtrip_exactly() {
        let fixture = fixture(history(BODY, 2));
        let entries = entries(&fixture.path);
        let objects: Vec<_> = entries
            .keys()
            .filter(|name| name.starts_with(OBJECT_PREFIX))
            .collect();
        assert_eq!(
            objects.len(),
            2,
            "one body object plus exact legacy binding bytes"
        );
        assert_eq!(
            entries[&format!("{OBJECT_PREFIX}{}", digest(BODY.as_bytes()).sha256)],
            BODY.as_bytes()
        );
        assert!(!entries.contains_key(project::HISTORY_ENTRY));
        assert!(!entries.contains_key(&format!("assets/{BINDINGS_ATTACHMENT}")));
        let reopened = Document::open(&fixture.path).unwrap();
        assert_eq!(reopened.history_archive(), &fixture.history);
        assert_eq!(reopened.attachments[BINDINGS_ATTACHMENT], BINDING_BYTES);
    }

    #[test]
    fn crc_valid_tampering_fails_snapshot_and_content_hash_checks() {
        let fixture = fixture(history(BODY, 1));
        let mut contents = entries(&fixture.path);
        let name = format!("{OBJECT_PREFIX}{}", digest(BODY.as_bytes()).sha256);
        contents.get_mut(&name).unwrap().fill(b'x');
        write_entries(&fixture.path, contents.clone(), None);
        // All entries, including the modified object, pass the rewritten CRC.
        assert_eq!(entries(&fixture.path)[&name], contents[&name]);
        assert!(open_error(&fixture.path).contains("项目内容校验失败"));

        // Even a new outer checksum cannot make unchanged content references
        // identify different bytes; this is integrity, not authenticity.
        write_entries(&fixture.path, contents, Some(&fixture.history));
        assert!(open_error(&fixture.path).contains("内容对象名称与正文哈希不一致"));
    }

    #[test]
    fn missing_and_unreferenced_objects_are_rejected() {
        let fixture = fixture(history(BODY, 1));
        let original = entries(&fixture.path);
        let mut missing = original.clone();
        missing.remove(&format!(
            "{OBJECT_PREFIX}{}",
            digest(BODY.as_bytes()).sha256
        ));
        write_entries(&fixture.path, missing, Some(&fixture.history));
        assert!(Document::open(&fixture.path).is_err());

        let mut extra = original;
        let bytes = b"unreferenced extension content";
        extra.insert(
            format!("{OBJECT_PREFIX}{}", digest(bytes).sha256),
            bytes.to_vec(),
        );
        write_entries(&fixture.path, extra, Some(&fixture.history));
        assert!(open_error(&fixture.path).contains("未被协作历史或画布绑定引用"));
    }

    #[test]
    fn duplicate_legacy_facts_are_rejected_after_valid_sealing() {
        let fixture = fixture(history(BODY, 1));
        let original = entries(&fixture.path);
        for (name, bytes) in [
            (
                project::HISTORY_ENTRY.to_owned(),
                serde_json::to_vec(&fixture.history).unwrap(),
            ),
            (
                format!("assets/{BINDINGS_ATTACHMENT}"),
                BINDING_BYTES.to_vec(),
            ),
        ] {
            let mut contents = original.clone();
            contents.insert(name, bytes);
            write_entries(&fixture.path, contents, Some(&fixture.history));
            assert!(open_error(&fixture.path).contains("重复事实来源"));
        }
    }

    #[test]
    fn snapshot_selection_and_identity_must_match_the_saved_document() {
        let fixture = fixture(history(BODY, 2));
        let original = entries(&fixture.path);
        for selection in [
            vec!["workflow-0"],
            vec!["workflow-1", "workflow-0"],
            vec!["workflow-0", "workflow-0"],
        ] {
            let mut contents = original.clone();
            let mut snapshot: Snapshot = serde_json::from_slice(&contents[SNAPSHOT_ENTRY]).unwrap();
            snapshot.selected_workflow_ids = selection.into_iter().map(String::from).collect();
            contents.insert(
                SNAPSHOT_ENTRY.into(),
                serde_json::to_vec(&snapshot).unwrap(),
            );
            write_entries(&fixture.path, contents, None);
            assert!(open_error(&fixture.path).contains("所选历史与记录不一致"));
        }
        let mut contents = original;
        let mut snapshot: Snapshot = serde_json::from_slice(&contents[SNAPSHOT_ENTRY]).unwrap();
        snapshot.project_id = uuid::Uuid::new_v4().to_string();
        contents.insert(
            SNAPSHOT_ENTRY.into(),
            serde_json::to_vec(&snapshot).unwrap(),
        );
        write_entries(&fixture.path, contents, None);
        assert!(open_error(&fixture.path).contains("保存快照无效"));
    }

    #[test]
    fn declared_sizes_and_reference_sizes_are_enforced() {
        assert!(checksum(&b"abc"[..], 2).is_err());
        assert!(checksum(&b"abc"[..], 4).is_err());
        assert_eq!(checksum(&b"abc"[..], 3).unwrap(), digest(b"abc"));

        let fixture = fixture(history(BODY, 1));
        let mut contents = entries(&fixture.path);
        let mut records: History<ObjectRef> =
            rmp_serde::from_slice(&contents[RECORDS_ENTRY]).unwrap();
        records.workflows[0].goal.size += 1;
        contents.insert(
            RECORDS_ENTRY.into(),
            rmp_serde::to_vec_named(&records).unwrap(),
        );
        write_entries(&fixture.path, contents, Some(&fixture.history));
        assert!(open_error(&fixture.path).contains("内容对象校验失败"));
    }

    #[test]
    fn repeated_body_size_comparison_uses_the_same_zip_compression() {
        // Fixed ASCII evidence with low local repetition, reused across 32 body
        // fields. This is a size sample, not a timing or memory benchmark.
        let mut state = 0x71c4_32b9u32;
        let body: String = (0..65_536)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                char::from(b'!' + (state % 90) as u8)
            })
            .collect();
        let fixture = fixture(history(&body, 4));
        let mut v1 = entries(&fixture.path);
        v1.retain(|name, _| !managed(name));
        let mut manifest: Manifest = serde_json::from_slice(&v1[project::MANIFEST_ENTRY]).unwrap();
        manifest.schema_version = 1;
        manifest.required_features.clear();
        v1.insert(
            project::MANIFEST_ENTRY.into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        v1.insert(
            project::HISTORY_ENTRY.into(),
            serde_json::to_vec(&fixture.history).unwrap(),
        );
        v1.insert(
            format!("assets/{BINDINGS_ATTACHMENT}"),
            BINDING_BYTES.to_vec(),
        );
        let old_path = fixture.directory.path().join("schema1.mio");
        write_entries(&old_path, v1, None);
        assert_eq!(
            Document::open(&old_path).unwrap().history_archive(),
            &fixture.history
        );
        let v1_bytes = fs::metadata(&old_path).unwrap().len();
        let v2_bytes = fs::metadata(&fixture.path).unwrap().len();
        eprintln!(
            "MIO size sample: 4 workflows, 32 repeated 64 KiB body fields; Deflated in both formats; schema1={v1_bytes} bytes, schema2={v2_bytes} bytes"
        );
        assert!(v2_bytes < v1_bytes);
    }
}

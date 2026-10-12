//! SQLite MIO v3. Object-sized records and ordered, stable-ID graph relations.
//! The legacy stage layout is lossless compatibility metadata, not a ZIP blob.
use crate::{
    Graph, ProjectFile, Result,
    project::{self, Manifest},
    project_history::{ArchivedRun, ArchivedWorkflow, History},
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params, types::Value as Sql};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

pub(crate) const MAGIC: &[u8] = b"SQLite format 3\0";
const APPLICATION_ID: i64 = 0x4d494f33; // MIO3
const MAX_BYTES: usize = 512 * 1024 * 1024;
const MAX_OBJECT: usize = 64 * 1024 * 1024;
const MAX_ROWS: usize = 300_000;

const SCHEMA: &str = include_str!("project_sqlite.sql");

struct Table {
    name: &'static str,
    columns: &'static [&'static str],
    keys: usize,
}

// Key columns come first. Identifiers are compile-time constants, never file data.
const TABLES: &[Table] = &[
    Table {
        name: "objects",
        columns: &["hash", "data"],
        keys: 1,
    },
    Table {
        name: "canvas_root",
        columns: &["id", "body"],
        keys: 1,
    },
    Table {
        name: "canvas_records",
        columns: &["id", "path", "kind", "body"],
        keys: 1,
    },
    Table {
        name: "canvas_nodes",
        columns: &["id"],
        keys: 1,
    },
    Table {
        name: "canvas_edges",
        columns: &["id"],
        keys: 1,
    },
    Table {
        name: "edge_endpoints",
        columns: &["edge_id", "ordinal", "node_id", "rate_x", "rate_y"],
        keys: 2,
    },
    Table {
        name: "group_members",
        columns: &["parent_id", "ordinal", "child_id"],
        keys: 2,
    },
    Table {
        name: "workflows",
        columns: &["id", "ordinal", "body"],
        keys: 1,
    },
    Table {
        name: "runs",
        columns: &["id", "workflow_id", "ordinal", "body", "retry_of"],
        keys: 1,
    },
    Table {
        name: "run_sources",
        columns: &["run_id", "ordinal", "source_id"],
        keys: 2,
    },
    Table {
        name: "attachments",
        columns: &["name", "body"],
        keys: 1,
    },
    Table {
        name: "extensions",
        columns: &["name", "body"],
        keys: 1,
    },
];

type Rows = BTreeMap<Vec<String>, Vec<Sql>>;
#[derive(Default, PartialEq)]
struct Image(BTreeMap<&'static str, Rows>);

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn text(value: &Sql) -> Result<&str> {
    match value {
        Sql::Text(s) => Ok(s),
        _ => Err("项目记录字段类型无效".into()),
    }
}
fn integer(value: &Sql) -> Result<i64> {
    match value {
        Sql::Integer(n) => Ok(*n),
        _ => Err("项目记录序号无效".into()),
    }
}
fn bytes(value: &Sql) -> Result<&[u8]> {
    match value {
        Sql::Blob(b) => Ok(b),
        _ => Err("项目内容对象类型无效".into()),
    }
}
fn string(value: impl Into<String>) -> Sql {
    Sql::Text(value.into())
}
fn ordinal(index: usize) -> Sql {
    Sql::Integer(index as i64)
}

impl Image {
    fn insert(&mut self, table: &'static str, row: Vec<Sql>) -> Result<()> {
        let spec = TABLES
            .iter()
            .find(|spec| spec.name == table)
            .ok_or("未知项目表")?;
        let key = row[..spec.keys]
            .iter()
            .map(|v| match v {
                Sql::Text(s) => Ok(s.clone()),
                Sql::Integer(n) => Ok(n.to_string()),
                _ => Err("项目主键类型无效".into()),
            })
            .collect::<Result<Vec<_>>>()?;
        if self.0.entry(table).or_default().insert(key, row).is_some() {
            return Err(format!("项目中存在重复身份：{table}").into());
        }
        Ok(())
    }
    fn rows(&self, table: &str) -> impl Iterator<Item = &Vec<Sql>> {
        self.0.get(table).into_iter().flat_map(|rows| rows.values())
    }
    fn put(&mut self, data: Vec<u8>) -> Result<String> {
        if data.len() > MAX_OBJECT {
            return Err("单个项目内容超过 64 MiB".into());
        }
        let id = hash(&data);
        if !self
            .0
            .get("objects")
            .is_some_and(|rows| rows.contains_key(&vec![id.clone()]))
        {
            self.insert("objects", vec![string(&id), Sql::Blob(data)])?;
        }
        Ok(id)
    }
    fn object(&self, id: &Sql) -> Result<&[u8]> {
        let row = self
            .0
            .get("objects")
            .and_then(|rows| rows.get(&vec![text(id).ok()?.to_owned()]))
            .ok_or("缺失项目内容对象")?;
        bytes(&row[1])
    }
    fn read(conn: &Connection) -> Result<Self> {
        let mut image = Self::default();
        let mut total = 0usize;
        let mut count = 0;
        for table in TABLES {
            let mut stmt = conn.prepare(&format!(
                "SELECT {} FROM {}",
                table.columns.join(","),
                table.name
            ))?;
            let mut cursor = stmt.query([])?;
            while let Some(row) = cursor.next()? {
                count += 1;
                if count > MAX_ROWS {
                    return Err("项目记录数量超过限制".into());
                }
                let mut values = Vec::new();
                for index in 0..table.columns.len() {
                    let value: Sql = row.get(index)?;
                    total += match &value {
                        Sql::Text(s) => s.len(),
                        Sql::Blob(b) => b.len(),
                        _ => 8,
                    };
                    if total > MAX_BYTES {
                        return Err("项目数据超过 512 MiB".into());
                    }
                    values.push(value);
                }
                image.insert(table.name, values)?;
            }
        }
        for row in image.rows("objects") {
            if hash(bytes(&row[1])?) != text(&row[0])? {
                return Err("项目内容哈希校验失败".into());
            }
        }
        Ok(image)
    }
    fn sync(&self, conn: &Connection) -> Result<()> {
        for table in TABLES {
            let name = table.name;
            let columns = table.columns.join(",");
            conn.execute_batch(&format!(
                "CREATE TEMP TABLE incoming AS SELECT {columns} FROM {name} WHERE 0"
            ))?;
            {
                let slots = vec!["?"; table.columns.len()].join(",");
                let mut stmt = conn.prepare(&format!("INSERT INTO incoming VALUES ({slots})"))?;
                for row in self.rows(name) {
                    stmt.execute(rusqlite::params_from_iter(row))?;
                }
            }
            let same_key = table.columns[..table.keys]
                .iter()
                .map(|c| format!("incoming.{c} = {name}.{c}"))
                .collect::<Vec<_>>()
                .join(" AND ");
            let keys = table.columns[..table.keys].join(",");
            conn.execute_batch(&format!(
                "CREATE UNIQUE INDEX incoming_key ON incoming ({keys})"
            ))?;
            conn.execute_batch(&format!(
                "DELETE FROM {name} WHERE NOT EXISTS (SELECT 1 FROM incoming WHERE {same_key})"
            ))?;
            let updates = &table.columns[table.keys..];
            let action = if updates.is_empty() {
                "NOTHING".to_owned()
            } else {
                let set = updates
                    .iter()
                    .map(|c| format!("{c}=excluded.{c}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let changed = updates
                    .iter()
                    .map(|c| format!("{name}.{c} IS NOT excluded.{c}"))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                format!("UPDATE SET {set} WHERE {changed}")
            };
            conn.execute_batch(&format!("INSERT INTO {name} ({columns}) SELECT {columns} FROM incoming WHERE true ON CONFLICT ({keys}) DO {action}; DROP TABLE incoming;"))?;
        }
        Ok(())
    }
}

pub(crate) struct Loaded {
    pub file: ProjectFile,
    pub manifest: Manifest,
    pub history: History,
    pub extensions: BTreeMap<String, Vec<u8>>,
    pub disk_bytes: Vec<u8>,
}

pub(crate) fn is_database(path: &Path) -> Result<bool> {
    let mut header = [0; 16];
    let n = fs::File::open(path)?.read(&mut header)?;
    Ok(n == 16 && header == MAGIC)
}

fn open(path: &Path, writable: bool) -> Result<Connection> {
    if fs::metadata(path)?.len() > (MAX_BYTES * 2) as u64 {
        return Err("项目文件超过大小限制".into());
    }
    let flags = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let conn = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    conn.busy_timeout(Duration::from_secs(2))?;
    conn.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
        MAX_OBJECT as i32 + 4096,
    )?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF;")?;
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    // v3 uses short rollback-journal transactions; a portable baseline is the
    // main file. Do not accidentally ignore a third-party WAL sidecar.
    if mode != "delete" {
        return Err("当前 .mio 版本需要 DELETE 日志模式，请导出独立项目副本".into());
    }
    Ok(conn)
}

fn read_contents(conn: &Connection) -> Result<Loaded> {
    let app: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if app != APPLICATION_ID || version != project::SQLITE_SCHEMA_VERSION as i64 {
        return Err("不支持的 SQLite 项目标识或版本".into());
    }
    let manifest_bytes: Vec<u8> =
        conn.query_row("SELECT manifest FROM project_meta WHERE id=1", [], |r| {
            r.get(0)
        })?;
    if manifest_bytes.len() > 65536 {
        return Err("项目清单超出大小限制".into());
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    manifest.validate()?;
    if manifest.schema_version != project::SQLITE_SCHEMA_VERSION {
        return Err("项目清单与数据库版本不一致".into());
    }
    let image = Image::read(conn)?;
    let root = image.rows("canvas_root").next().ok_or("缺少画布结构")?;
    let mut stage: Value = serde_json::from_slice(image.object(&root[1])?)?;
    let mut records = image.rows("canvas_records").collect::<Vec<_>>();
    records.sort_by_key(|row| text(&row[1]).unwrap_or("").matches('/').count());
    for row in records {
        let path = text(&row[1])?;
        if path.is_empty() {
            return Err("画布对象路径为空".into());
        }
        let slot = stage.pointer_mut(path).ok_or("画布对象布局引用不存在")?;
        if !slot.is_null() {
            return Err("画布对象布局重复".into());
        }
        *slot = serde_json::from_slice(image.object(&row[3])?)?;
    }
    let blobs = |table| -> Result<BTreeMap<String, Vec<u8>>> {
        image
            .rows(table)
            .map(|row| Ok((text(&row[0])?.to_owned(), image.object(&row[1])?.to_vec())))
            .collect()
    };
    let attachments = blobs("attachments")?;
    let extensions = blobs("extensions")?;
    let mut runs_by_workflow: BTreeMap<&str, Vec<&Vec<Sql>>> = BTreeMap::new();
    let mut sources_by_run: BTreeMap<&str, Vec<&Vec<Sql>>> = BTreeMap::new();
    for row in image.rows("runs") {
        runs_by_workflow
            .entry(text(&row[1])?)
            .or_default()
            .push(row);
    }
    for row in image.rows("run_sources") {
        sources_by_run.entry(text(&row[0])?).or_default().push(row);
    }
    let mut workflows = Vec::new();
    for row in image.rows("workflows") {
        let mut workflow: ArchivedWorkflow<String> =
            serde_json::from_slice(image.object(&row[2])?)?;
        let mut runs = Vec::new();
        for run_row in runs_by_workflow.get(text(&row[0])?).into_iter().flatten() {
            let mut run: ArchivedRun<String> = serde_json::from_slice(image.object(&run_row[3])?)?;
            let mut sources = sources_by_run
                .get(text(&run_row[0])?)
                .into_iter()
                .flatten()
                .map(|r| Ok((integer(&r[1])?, text(&r[2])?.to_owned())))
                .collect::<Result<Vec<_>>>()?;
            sources.sort_by_key(|(order, _)| *order);
            run.sources = sources.into_iter().map(|(_, id)| id).collect();
            run.retry_of = match &run_row[4] {
                Sql::Null => None,
                v => Some(text(v)?.to_owned()),
            };
            runs.push((integer(&run_row[2])?, run));
        }
        runs.sort_by_key(|(order, _)| *order);
        workflow.runs = runs.into_iter().map(|(_, run)| run).collect();
        workflows.push((integer(&row[1])?, workflow));
    }
    workflows.sort_by_key(|(order, _)| *order);
    let history = History {
        schema_version: if workflows
            .iter()
            .any(|(_, w)| w.runs.iter().any(|r| r.delegation.is_some()))
        {
            2
        } else {
            1
        },
        workflows: workflows.into_iter().map(|(_, w)| w).collect(),
    };
    // Resolve content hashes only in body fields, retaining identities and order.
    let mut expanded = 0usize;
    let history = history.try_map_text(|id| {
        let data = image.object(&string(id))?;
        expanded += data.len();
        if expanded > MAX_OBJECT {
            return Err("项目协作正文展开后超过 64 MiB".into());
        }
        Ok(String::from_utf8(data.to_vec())?)
    })?;
    let file = ProjectFile { stage, attachments };
    // Rebuild the canonical relational representation. This validates IDs,
    // ordered endpoints (including self-loops), derived indexes and object closure.
    if encode(&file, &history, &extensions)? != image {
        return Err("项目关系、记录或内容闭包校验不一致".into());
    }
    Ok(Loaded {
        file,
        manifest,
        history,
        extensions,
        disk_bytes: vec![],
    })
}

pub(crate) fn load(path: &Path) -> Result<Loaded> {
    match load_with_access(path, false) {
        Err(error)
            if matches!(error.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(code, _))
                if code.extended_code == rusqlite::ffi::SQLITE_READONLY_ROLLBACK) =>
        {
            // A killed writer can leave a hot journal. SQLite must roll back
            // those uncommitted pages before any reader can see the document.
            load_with_access(path, true)
        }
        result => result,
    }
}

fn load_with_access(path: &Path, writable: bool) -> Result<Loaded> {
    let mut conn = open(path, writable)?;
    let tx = conn.transaction()?;
    let mut loaded = read_contents(&tx)?;
    loaded.disk_bytes = fs::read(path)?; // shared read lock, DELETE journal mode
    tx.commit()?;
    Ok(loaded)
}

fn encode(
    file: &ProjectFile,
    history: &History,
    extensions: &BTreeMap<String, Vec<u8>>,
) -> Result<Image> {
    history.validate()?;
    let mut image = Image::default();
    fn split(value: &mut Value, path: &str, image: &mut Image, depth: usize) -> Result<()> {
        if depth > 128 {
            return Err("画布结构嵌套过深".into());
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    split(
                        child,
                        &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                        image,
                        depth + 1,
                    )?;
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter_mut().enumerate() {
                    split(child, &format!("{path}/{index}"), image, depth + 1)?;
                }
            }
            _ => {}
        }
        if let Some(id) = value.get("uuid").and_then(Value::as_str) {
            if id.is_empty() {
                return Err("画布对象身份为空".into());
            }
            let body = image.put(serde_json::to_vec(value)?)?;
            image.insert(
                "canvas_records",
                vec![
                    string(id),
                    string(path),
                    string(value["_"].as_str().ok_or("画布对象缺少类型")?),
                    string(body),
                ],
            )?;
            *value = Value::Null;
        }
        Ok(())
    }
    let mut root = file.stage.clone();
    if !root.is_array() {
        return Err("画布根必须是数组".into());
    }
    split(&mut root, "", &mut image, 0)?;
    let body = image.put(serde_json::to_vec(&root)?)?;
    image.insert("canvas_root", vec![Sql::Integer(1), string(body)])?;
    let graph = Graph::from_file(ProjectFile {
        stage: file.stage.clone(),
        attachments: file.attachments.clone(),
    })?;
    let mut parents = BTreeMap::new();
    for node in &graph.nodes {
        image.insert("canvas_nodes", vec![string(&node.id)])?;
        for (index, child) in node.children.iter().enumerate() {
            if parents.insert(child, &node.id).is_some() {
                return Err("分组成员存在多个父组".into());
            }
            image.insert(
                "group_members",
                vec![string(&node.id), ordinal(index), string(child)],
            )?;
        }
    }
    let mut checked = BTreeSet::new();
    for node in &graph.nodes {
        let mut id = &node.id;
        let mut trail = BTreeSet::new();
        loop {
            if checked.contains(id) {
                break;
            }
            let Some(parent) = parents.get(id) else {
                break;
            };
            if !trail.insert(id) {
                return Err("分组关系存在循环".into());
            }
            id = parent;
        }
        checked.extend(trail);
    }
    for edge in &graph.edges {
        image.insert("canvas_edges", vec![string(&edge.id)])?;
        for (index, (&member, rate)) in edge.members.iter().zip(&edge.rates).enumerate() {
            image.insert(
                "edge_endpoints",
                vec![
                    string(&edge.id),
                    ordinal(index),
                    string(&graph.nodes[member].id),
                    Sql::Real(rate[0] as f64),
                    Sql::Real(rate[1] as f64),
                ],
            )?;
        }
    }
    let mut expanded = 0usize;
    let records = history.clone().try_map_text(|body| {
        expanded += body.len();
        if expanded > MAX_OBJECT {
            return Err("项目协作正文展开后超过 64 MiB".into());
        }
        image.put(body.into_bytes())
    })?;
    for (index, mut workflow) in records.workflows.into_iter().enumerate() {
        for (run_index, mut run) in std::mem::take(&mut workflow.runs).into_iter().enumerate() {
            for (source_index, source) in std::mem::take(&mut run.sources).into_iter().enumerate() {
                image.insert(
                    "run_sources",
                    vec![string(&run.id), ordinal(source_index), string(source)],
                )?;
            }
            let retry = run.retry_of.take().map(string).unwrap_or(Sql::Null);
            let body = image.put(serde_json::to_vec(&run)?)?;
            image.insert(
                "runs",
                vec![
                    string(&run.id),
                    string(&workflow.id),
                    ordinal(run_index),
                    string(body),
                    retry,
                ],
            )?;
        }
        let body = image.put(serde_json::to_vec(&workflow)?)?;
        image.insert(
            "workflows",
            vec![string(&workflow.id), ordinal(index), string(body)],
        )?;
    }
    for (table, entries) in [
        ("attachments", &file.attachments),
        ("extensions", extensions),
    ] {
        for (name, data) in entries {
            project::validate_entry_name(name)?;
            if table == "extensions"
                && (matches!(
                    name.as_str(),
                    project::MANIFEST_ENTRY | project::HISTORY_ENTRY
                ) || name == project::Format::Mio.stage()
                    || name.starts_with(project::Format::Mio.assets())
                    || crate::project_package::managed(name))
            {
                return Err("扩展占用项目保留路径".into());
            }
            let body = image.put(data.clone())?;
            image.insert(table, vec![string(name), string(body)])?;
        }
    }
    let count: usize = image.0.values().map(BTreeMap::len).sum();
    let total: usize = image
        .0
        .values()
        .flat_map(|rows| rows.values())
        .flatten()
        .map(|v| match v {
            Sql::Blob(b) => b.len(),
            Sql::Text(s) => s.len(),
            _ => 8,
        })
        .sum();
    if count > MAX_ROWS || total > MAX_BYTES {
        return Err("项目超过记录或大小限制".into());
    }
    Ok(image)
}

pub(crate) struct Save<'a> {
    pub file: &'a ProjectFile,
    pub manifest: &'a Manifest,
    pub history: &'a History,
    pub extensions: &'a BTreeMap<String, Vec<u8>>,
    pub revision: u64,
    pub expected: Option<&'a [u8]>,
}

pub(crate) fn save(path: &Path, replace: bool, data: Save<'_>) -> Result<Vec<u8>> {
    let manifest = data.manifest.for_database();
    manifest.validate()?;
    let encoded = encode(data.file, data.history, data.extensions)?;
    let update = replace && path.exists() && is_database(path)?;
    let temp = if update {
        None
    } else {
        Some(tempfile::NamedTempFile::new_in(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?)
    };
    let target = temp.as_ref().map_or(path, |t| t.path());
    let mut conn = open(target, true)?;
    conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA locking_mode=EXCLUSIVE;")?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Exclusive)?;
    if update {
        read_contents(&tx)?;
        if let Some(expected) = data.expected
            && fs::read(path)? != expected
        {
            return Err("磁盘项目已被其他程序更改，请另存为以保留两个版本".into());
        }
    } else {
        tx.execute_batch(SCHEMA)?;
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.pragma_update(None, "user_version", project::SQLITE_SCHEMA_VERSION)?;
    }
    encoded.sync(&tx)?;
    tx.execute("INSERT INTO project_meta(id,manifest,revision) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET manifest=excluded.manifest, revision=excluded.revision WHERE manifest IS NOT excluded.manifest OR revision IS NOT excluded.revision",
        params![serde_json::to_vec(&manifest)?, i64::try_from(data.revision)?])?;
    let checked = read_contents(&tx)?;
    if checked.manifest != manifest
        || checked.file.stage != data.file.stage
        || checked.file.attachments != data.file.attachments
        || checked.history != *data.history
        || checked.extensions != *data.extensions
    {
        return Err("数据库保存往返校验不一致，事务已回滚".into());
    }
    let violations: i64 =
        tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })?;
    if violations != 0 {
        return Err("项目存在悬空关系，事务已回滚".into());
    }
    tx.commit()?;
    let output = fs::read(target)?; // exclusive locking mode remains held until close
    conn.close().map_err(|(_, e)| e)?;
    if let Some(temp) = temp {
        temp.as_file().sync_all()?;
        if replace {
            temp.persist(path)?;
        } else {
            temp.persist_noclobber(path)?;
        }
    }
    Ok(output)
}

impl Loaded {
    /// Existing canvas edit codecs retain their lossless stage baseline. This
    /// transient v1 adapter is never persisted in the SQLite database.
    pub(crate) fn compatibility_archive(&self) -> Result<Vec<u8>> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let mut manifest = self.manifest.clone();
        manifest.schema_version = 1;
        manifest.required_features.clear();
        let mut entries = self.extensions.clone();
        entries.insert(
            project::MANIFEST_ENTRY.into(),
            serde_json::to_vec(&manifest)?,
        );
        entries.insert(
            project::HISTORY_ENTRY.into(),
            serde_json::to_vec(&self.history)?,
        );
        entries.insert(
            project::Format::Mio.stage().into(),
            rmp_serde::to_vec(&self.file.stage)?,
        );
        for (name, bytes) in &self.file.attachments {
            entries.insert(format!("assets/{name}"), bytes.clone());
        }
        for (name, bytes) in entries {
            writer.start_file(name, zip::write::SimpleFileOptions::default())?;
            writer.write_all(&bytes)?;
        }
        Ok(writer.finish()?.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphEdge, document::Document};
    use std::collections::HashSet;

    fn nodes(doc: &Document) -> BTreeMap<&str, &crate::GraphNode> {
        doc.nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect()
    }

    #[test]
    fn fractional_colors_keep_content_hashes_after_json_roundtrip() {
        let file = ProjectFile {
            stage: serde_json::json!([{
                "_": "TextNode", "uuid": "fractional-color", "text": "色彩",
                "collisionBox": {"shapes": [{"_": "Rectangle",
                    "location": {"x": 0, "y": 0}, "size": {"x": 100, "y": 80}}]},
                "color": {"r": 0.23_f32 as f64 * 255.0,
                          "g": 0.47_f32 as f64 * 255.0,
                          "b": 0.79_f32 as f64 * 255.0,
                          "a": 0.8_f32 as f64}
            }]),
            attachments: BTreeMap::new(),
        };
        let encoded = encode(&file, &History::default(), &BTreeMap::new()).unwrap();
        let stage = serde_json::from_slice(&serde_json::to_vec(&file.stage).unwrap()).unwrap();
        let decoded = ProjectFile {
            stage,
            attachments: BTreeMap::new(),
        };
        assert!(encoded == encode(&decoded, &History::default(), &BTreeMap::new()).unwrap());
    }

    #[test]
    fn interrupted_writer_child() {
        let Some(path) = std::env::var_os("RAVOMI_TEST_SQLITE_CRASH_PATH") else {
            return;
        };
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "PRAGMA synchronous=FULL; BEGIN EXCLUSIVE; UPDATE objects SET data=x'000102';",
        )
        .unwrap();
        conn.cache_flush().unwrap();
        // Exit without running Connection/Transaction destructors, after dirty
        // database pages reached disk but before the commit marker was written.
        std::process::exit(0);
    }

    #[test]
    fn interrupted_transaction_recovers_committed_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.mio");
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.save(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "project_sqlite::tests::interrupted_writer_child",
                "--nocapture",
            ])
            .env("RAVOMI_TEST_SQLITE_CRASH_PATH", &path)
            .status()
            .unwrap();
        assert!(status.success());
        assert_ne!(fs::read(&path).unwrap(), before);
        assert_eq!(nodes(&Document::open(&path).unwrap()), nodes(&doc));
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn ordered_hyperedge_endpoints_and_shared_run_bodies_roundtrip() {
        use crate::project_history::{ArchivedRun, ArchivedStatus, ArchivedWorkflow};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.mio");
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.create_text_node([200.0; 2]).unwrap();
        let source = dir.path().join("source.mio");
        doc.save(&source).unwrap();
        let mut file = ProjectFile::load(&source).unwrap();
        file.stage.as_array_mut().unwrap().push(serde_json::json!({
            "_": "MultiTargetUndirectedEdge", "uuid": "hyperedge",
            "associationList": [{"$":"/1"},{"$":"/0"},{"$":"/1"}],
            "rectRates": [{"x":0.2,"y":0.3},{"x":0.4,"y":0.5},{"x":0.6,"y":0.7}]
        }));
        let body = "same evidence shared across inputs and outputs";
        let run = |id: &str, sources: Vec<String>, retry_of: Option<String>| ArchivedRun {
            id: id.into(),
            task_id: "$plan".into(),
            status: ArchivedStatus::Succeeded,
            created_at: 1,
            finished_at: Some(2),
            sources,
            retry_of,
            delegation: None,
            delegated_results: vec![],
            prompt: body.into(),
            input_files: vec![],
            output: body.into(),
            proposal: None,
            error: None,
            review: None,
            usage: None,
        };
        let history = History {
            schema_version: 1,
            workflows: vec![ArchivedWorkflow {
                id: "workflow".into(),
                goal: body.into(),
                model: "test/model".into(),
                files: vec![],
                plan: None,
                draft_plan: None,
                runs: vec![
                    run("first", vec![], None),
                    run("retry", vec!["first".into()], Some("first".into())),
                ],
            }],
        };
        let manifest = Manifest::default();
        let extensions = BTreeMap::new();
        save(
            &path,
            true,
            Save {
                file: &file,
                manifest: &manifest,
                history: &history,
                extensions: &extensions,
                revision: 0,
                expected: None,
            },
        )
        .unwrap();
        let saved = load(&path).unwrap();
        assert_eq!(saved.file.stage, file.stage);
        assert_eq!(saved.history, history);
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM objects WHERE data=?",
                [body.as_bytes()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let endpoints: Vec<String> = conn
            .prepare(
                "SELECT node_id FROM edge_endpoints WHERE edge_id='hyperedge' ORDER BY ordinal",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            endpoints,
            [
                doc.nodes[1].id.clone(),
                doc.nodes[0].id.clone(),
                doc.nodes[1].id.clone()
            ]
        );
        // Exercise the lower-level check after a writer won the race, bypassing
        // Document's early convenience check. The acquired lock must protect it.
        let stale = saved.disk_bytes;
        conn.execute("UPDATE project_meta SET revision=99", [])
            .unwrap();
        drop(conn);
        let current = fs::read(&path).unwrap();
        assert!(
            save(
                &path,
                true,
                Save {
                    file: &file,
                    manifest: &manifest,
                    history: &history,
                    extensions: &extensions,
                    revision: 1,
                    expected: Some(&stale)
                }
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), current);
    }

    #[test]
    fn graph_relations_assets_and_undo_survive_repeated_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.mio");
        let mut doc = Document::blank().unwrap();
        let a = doc.create_text_node([0.0, 0.0]).unwrap();
        let b = doc.create_text_node([200.0, 0.0]).unwrap();
        let id_a = doc.nodes[a].id.clone();
        doc.add_edge(GraphEdge {
            id: "loop".into(),
            members: vec![a, a],
            rates: vec![[0.2, 0.3], [0.8, 0.7]],
            ..Default::default()
        })
        .unwrap();
        for id in ["parallel-a", "parallel-b"] {
            doc.add_edge(GraphEdge {
                id: id.into(),
                members: vec![a, b],
                rates: vec![[0.5; 2]; 2],
                ..Default::default()
            })
            .unwrap();
        }
        doc.attachments
            .insert("evidence.txt".into(), b"unchanged evidence".to_vec());
        doc.save(&path).unwrap();
        assert!(is_database(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        let endpoints: Vec<String> = conn
            .prepare("SELECT node_id FROM edge_endpoints WHERE edge_id='loop' ORDER BY ordinal")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(endpoints, [id_a.clone(), id_a]);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM canvas_edges", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
        drop(conn);
        let baseline = fs::read(&path).unwrap();
        doc.save(&path).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            baseline,
            "no-op save does not rewrite pages"
        );
        doc.create_group(&HashSet::from([a, b])).unwrap();
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert_eq!(nodes(&reopened), nodes(&doc));
        assert_eq!(reopened.attachments, doc.attachments);
        assert_eq!(reopened.edges.len(), 3);
        doc.undo();
        doc.save(&path).unwrap();
        assert_eq!(Document::open(&path).unwrap().nodes.len(), 2);
        let snapshot = dir.path().join("recovery.mio");
        doc.save_snapshot(&snapshot).unwrap();
        assert!(doc.save_snapshot(&snapshot).is_err());
        assert_eq!(nodes(&Document::open(&snapshot).unwrap()), nodes(&doc));
    }

    #[test]
    fn changed_node_updates_only_its_record_and_keeps_file_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("incremental.mio");
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.create_text_node([200.0; 2]).unwrap();
        doc.attachments
            .insert("large.bin".into(), vec![5; 1024 * 1024]);
        doc.save(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE audit(name TEXT); CREATE TRIGGER node_changed AFTER UPDATE ON canvas_records BEGIN INSERT INTO audit VALUES('node'); END; CREATE TRIGGER asset_changed AFTER UPDATE ON attachments BEGIN INSERT INTO audit VALUES('asset'); END;").unwrap();
        drop(conn);
        let mut doc = Document::open(&path).unwrap();
        let id = doc.nodes[0].id.clone();
        #[cfg(unix)]
        let inode = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&path).unwrap().ino()
        };
        doc.edit_text(&id, "只改一个节点".into(), [100.0, 80.0]);
        doc.save(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        }
        let conn = Connection::open(&path).unwrap();
        let changes: Vec<String> = conn
            .prepare("SELECT name FROM audit")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(changes, ["node"]);
    }

    #[test]
    fn transaction_failure_and_stale_writer_preserve_committed_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollback.mio");
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.save(&path).unwrap();
        let mut stale = Document::open(&path).unwrap();
        doc.create_text_node([100.0; 2]).unwrap();
        doc.save(&path).unwrap();
        stale.create_text_node([200.0; 2]).unwrap();
        assert!(stale.save(&path).is_err());
        assert!(stale.dirty());
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TRIGGER reject_save BEFORE UPDATE ON project_meta BEGIN SELECT RAISE(ABORT,'injected write failure'); END;").unwrap();
        drop(conn);
        let mut doc = Document::open(&path).unwrap();
        let before = fs::read(&path).unwrap();
        doc.create_text_node([300.0; 2]).unwrap();
        assert!(doc.save(&path).is_err());
        assert!(doc.dirty());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(Document::open(&path).unwrap().nodes.len(), 2);
    }

    #[test]
    fn corruption_future_versions_and_unknown_requirements_are_rejected() {
        for sql in [
            "UPDATE objects SET data=x'000102'",
            "PRAGMA user_version=999",
            "PRAGMA application_id=0",
            "UPDATE project_meta SET manifest=CAST(json_set(CAST(manifest AS TEXT),'$.requiredFeatures',json('[\"future\"]')) AS BLOB)",
            "UPDATE canvas_records SET path='/missing'",
            "DELETE FROM canvas_nodes",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("invalid.mio");
            let mut doc = Document::blank().unwrap();
            doc.create_text_node([0.0; 2]).unwrap();
            doc.save(&path).unwrap();
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
            conn.execute_batch(sql).unwrap();
            drop(conn);
            let before = fs::read(&path).unwrap();
            assert!(Document::open(&path).is_err(), "{sql}");
            assert!(doc.save(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn zip_v2_migrates_only_on_save_and_can_export_plain_canvas() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.mio");
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.write_archive(&path, true, false).unwrap();
        let old = fs::read(&path).unwrap();
        let mut opened = Document::open(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), old);
        opened.save(&path).unwrap();
        assert!(is_database(&path).unwrap());
        let mut native = Document::open(&path).unwrap();
        native.save(&dir.path().join("export.mio")).unwrap();
        assert_eq!(
            Document::open(&dir.path().join("export.mio"))
                .unwrap()
                .nodes,
            native.nodes
        );
    }
}

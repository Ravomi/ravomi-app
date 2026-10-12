//! Code collaboration is independent of the drawing document and renderer.
//! Every workflow owns a fixed source snapshot; a run never writes the source project.
pub mod canvas;
mod pi;
mod runtime;
pub mod skills;
pub use runtime::Runtime;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, String>;
pub const MAX_BYTES: usize = 512 * 1024;
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub role: String,
    pub instruction: String,
    pub acceptance: String,
    pub dependencies: Vec<String>,
    pub parent_agent: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Proposal {
    pub summary: String,
    pub files: Vec<SourceFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Running,
    Stopping,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
    StorageBlocked,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub task_id: String,
    pub status: Status,
    pub created_at: u64,
    pub finished_at: Option<u64>,
    pub sources: Vec<String>,
    pub prompt: String,
    pub input_files: Vec<SourceFile>,
    pub output: String,
    pub proposal: Option<Proposal>,
    pub error: Option<String>,
    pub review: Option<String>,
    pub usage: Option<serde_json::Value>,
    pub session_id: Option<String>,
    pub retry_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<Delegation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delegated_results: Vec<String>,
}

/// A live delegation is not a dependency on the unfinished parent's output.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Delegation {
    pub parent_run_id: String,
    pub call_id: String,
    pub request: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub goal: String,
    pub project: String,
    pub model: String,
    pub files: Vec<SourceFile>,
    pub test_command: Vec<String>,
    pub plan: Option<Plan>,
    pub draft_plan: Option<Plan>,
    pub runs: Vec<Run>,
    pub paused: bool,
    pub max_calls: usize,
    pub timeout_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: u32,
    pub workflows: Vec<Workflow>,
    pub storage_error: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            version: 1,
            workflows: vec![],
            storage_error: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    CreateSkill {
        project: String,
        model: String,
        name: String,
        sources: Vec<skills::RunRef>,
    },
    Create {
        project: String,
        paths: Vec<String>,
        goal: String,
        model: String,
        test_command: Vec<String>,
    },
    SetPlan {
        id: String,
        plan: Plan,
    },
    Start {
        id: String,
    },
    Pause {
        id: String,
    },
    Stop {
        id: String,
        task_id: Option<String>,
    },
    Retry {
        id: String,
        run_id: String,
    },
    FollowUp {
        id: String,
        task_id: String,
        message: String,
    },
    Resolve {
        id: String,
        run_id: String,
    },
    Review {
        id: String,
        run_id: String,
        decision: String,
    },
    Test {
        id: String,
        run_id: String,
    },
    Flush,
}

fn valid_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 240
        || path.contains(['\\', ':', '\0', '\n', '\r'])
        || Path::new(path)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
        || path
            .split('/')
            .any(|p| p.starts_with('.') || matches!(p, "node_modules" | "target"))
    {
        return Err(format!("仅允许项目内非隐藏相对路径：{path}"));
    }
    Ok(())
}

fn source_path(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    valid_path(relative)?;
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        path.push(part);
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err(format!("不读取符号链接：{relative}"));
        }
    }
    Ok(path)
}

pub fn scan_project(root: &Path) -> Result<Vec<String>> {
    fn walk(root: &Path, dir: &Path, output: &mut Vec<String>, depth: usize) -> Result<()> {
        if depth > 12 || output.len() > 1000 {
            return Err("项目过大，请选择更小的源码目录".into());
        }
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "target" | "node_modules" | "dist" | "vendor" | "results"
                )
            {
                continue;
            }
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                walk(root, &entry.path(), output, depth + 1)?;
            } else if kind.is_file()
                && entry.metadata().map_err(|e| e.to_string())?.len() <= 128 * 1024
            {
                let path = entry.path();
                let extension = path.extension().and_then(|v| v.to_str()).unwrap_or("");
                if matches!(
                    extension,
                    "rs" | "ts"
                        | "tsx"
                        | "js"
                        | "jsx"
                        | "mjs"
                        | "cjs"
                        | "py"
                        | "go"
                        | "c"
                        | "h"
                        | "cpp"
                        | "java"
                        | "kt"
                        | "swift"
                        | "toml"
                        | "json"
                        | "md"
                        | "css"
                        | "html"
                        | "lock"
                ) {
                    output.push(
                        path.strip_prefix(root)
                            .map_err(|e| e.to_string())?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        Ok(())
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut output = vec![];
    walk(&root, &root, &mut output, 0)?;
    output.sort();
    Ok(output)
}

fn read_sources(project: &str, paths: &[String]) -> Result<Vec<SourceFile>> {
    if paths.is_empty() || paths.len() > 64 {
        return Err("请选择 1–64 个源码文件".into());
    }
    let root = Path::new(project);
    let mut unique = BTreeSet::new();
    let mut files = vec![];
    for relative in paths {
        if !unique.insert(relative) {
            return Err("重复的输入文件".into());
        }
        let path = source_path(root, relative)?;
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 128 * 1024 {
            return Err("单文件超过 128 KiB".into());
        }
        let content = fs::read_to_string(path).map_err(|e| format!("{relative}: {e}"))?;
        files.push(SourceFile {
            path: relative.clone(),
            content,
        });
    }
    validate_files(&files)?;
    Ok(files)
}

fn validate_files(files: &[SourceFile]) -> Result<()> {
    if files.len() > 128 || files.iter().map(|f| f.content.len()).sum::<usize>() > MAX_BYTES {
        return Err("源码快照超过 128 个文件或 512 KiB".into());
    }
    let mut names = BTreeSet::new();
    for file in files {
        valid_path(&file.path)?;
        if !names.insert(file.path.to_lowercase()) {
            return Err("文件路径重复（含大小写冲突）".into());
        }
        if file.content.contains('\0') {
            return Err("仅支持 UTF-8 源码文本".into());
        }
    }
    for a in &names {
        if names.iter().any(|b| b.starts_with(&format!("{a}/"))) {
            return Err("文件与目录路径冲突".into());
        }
    }
    Ok(())
}

fn validate_plan(plan: &Plan) -> Result<()> {
    if plan.tasks.is_empty() {
        return Err("计划至少需要一个任务".into());
    }
    let ids: BTreeSet<_> = plan.tasks.iter().map(|t| t.id.as_str()).collect();
    if ids.len() != plan.tasks.len() {
        return Err("任务 ID 重复".into());
    }
    for task in &plan.tasks {
        if task.id.is_empty()
            || task.id.starts_with('$')
            || task.id.len() > 48
            || !task
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || task.title.trim().is_empty()
            || task.role.trim().is_empty()
            || task.instruction.trim().is_empty()
            || task.acceptance.trim().is_empty()
            || task.instruction.len() > 16_000
            || task.title.len() > 200
            || task.role.len() > 200
            || task.acceptance.len() > 4000
        {
            return Err("任务需要有效 ID、名称、职责、指令和验收条件".into());
        }
        if task
            .dependencies
            .iter()
            .any(|d| !ids.contains(d.as_str()) || d == &task.id)
            || task.dependencies.iter().collect::<BTreeSet<_>>().len() != task.dependencies.len()
        {
            return Err("依赖缺失、重复或指向自身".into());
        }
        if task
            .parent_agent
            .as_ref()
            .is_some_and(|p| p == &task.id || !ids.contains(p.as_str()))
        {
            return Err("子 Agent 的父代理不存在或指向自身".into());
        }
        let mut ancestry = BTreeSet::from([task.id.as_str()]);
        let mut parent = task.parent_agent.as_deref();
        while let Some(id) = parent {
            if !ancestry.insert(id) {
                return Err("代理委派存在环".into());
            }
            parent = plan
                .tasks
                .iter()
                .find(|t| t.id == id)
                .and_then(|t| t.parent_agent.as_deref());
        }
    }
    let mut visited = BTreeSet::new();
    loop {
        let before = visited.len();
        for task in &plan.tasks {
            if task.dependencies.iter().all(|d| visited.contains(d)) {
                visited.insert(task.id.clone());
            }
        }
        if visited.len() == plan.tasks.len() {
            return Ok(());
        }
        if before == visited.len() {
            return Err("执行依赖存在环".into());
        }
    }
}

fn latest<'a>(workflow: &'a Workflow, task: &str) -> Option<&'a Run> {
    workflow.runs.iter().rev().find(|r| r.task_id == task)
}

// Fold all transitive changes against ONE workflow baseline. Conflicting branches
// remain separate artifacts until an integration agent explicitly resolves them.
fn merge_inputs(workflow: &Workflow, sources: &[String]) -> Result<Vec<SourceFile>> {
    let mut files: BTreeMap<String, String> = workflow
        .files
        .iter()
        .map(|f| (f.path.clone(), f.content.clone()))
        .collect();
    let mut proposals: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for run_id in sources {
        let run = workflow
            .runs
            .iter()
            .find(|r| &r.id == run_id && r.status == Status::Succeeded)
            .ok_or("交接成果不可用")?;
        let proposal = run.proposal.as_ref().ok_or("交接缺少代码工件")?;
        let mut effective: BTreeMap<String, String> = run
            .input_files
            .iter()
            .map(|f| (f.path.clone(), f.content.clone()))
            .collect();
        effective.extend(
            proposal
                .files
                .iter()
                .map(|f| (f.path.clone(), f.content.clone())),
        );
        for (path, content) in effective {
            if files.get(&path) != Some(&content) {
                proposals.entry(path).or_default().insert(content);
            }
        }
    }
    for (path, values) in proposals {
        // The prompt supplies every competing version; don't silently choose one.
        if values.len() == 1 {
            files.insert(path, values.into_iter().next().ok_or("空工件")?);
        }
    }
    Ok(files
        .into_iter()
        .map(|(path, content)| SourceFile { path, content })
        .collect())
}

fn parse_json<T: serde::de::DeserializeOwned>(text: &str) -> Result<T> {
    let text = text.trim();
    let text = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .and_then(|v| v.strip_suffix("```"))
        .unwrap_or(text)
        .trim();
    serde_json::from_str(text).map_err(|e| format!("Agent 返回的结构不符合约定：{e}"))
}

fn write_files(root: &Path, files: &[SourceFile]) -> Result<()> {
    validate_files(files)?;
    for file in files {
        let path = root.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(path, &file.content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

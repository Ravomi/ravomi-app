//! Portable collaboration evidence, separate from the live execution store.
//!
//! These DTOs deliberately omit source-project bindings, session handles and
//! dispatch settings. Capturing or decoding them never starts or restores a Run.
//! Free-form evidence is preserved as supplied; this is not a content scrubber.
use crate::{Result, collaboration};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const HISTORY_ENTRY: &str = "collaboration/history.json";
pub const HISTORY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct History {
    pub schema_version: u32,
    pub workflows: Vec<ArchivedWorkflow>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            schema_version: HISTORY_SCHEMA_VERSION,
            workflows: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedWorkflow {
    pub id: String,
    pub goal: String,
    pub model: String,
    pub files: Vec<ArchivedFile>,
    pub plan: Option<ArchivedPlan>,
    pub draft_plan: Option<ArchivedPlan>,
    pub runs: Vec<ArchivedRun>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedPlan {
    pub tasks: Vec<ArchivedTask>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedTask {
    pub id: String,
    pub title: String,
    pub role: String,
    pub instruction: String,
    pub acceptance: String,
    pub dependencies: Vec<String>,
    pub parent_agent: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedProposal {
    pub summary: String,
    pub files: Vec<ArchivedFile>,
}

/// The status observed at capture time, never a request to resume execution.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchivedStatus {
    Queued,
    Running,
    Stopping,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
    StorageBlocked,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedRun {
    pub id: String,
    pub task_id: String,
    pub status: ArchivedStatus,
    pub created_at: u64,
    pub finished_at: Option<u64>,
    pub sources: Vec<String>,
    pub prompt: String,
    pub input_files: Vec<ArchivedFile>,
    pub output: String,
    pub proposal: Option<ArchivedProposal>,
    pub error: Option<String>,
    /// Historical evidence only; this does not authorize a local action.
    pub review: Option<String>,
    pub usage: Option<serde_json::Value>,
    pub retry_of: Option<String>,
}

impl History {
    /// Capture only the explicitly selected workflows, in the supplied order.
    /// No matching by filesystem path, current document or global UI selection.
    pub fn capture(snapshot: &collaboration::Snapshot, workflow_ids: &[String]) -> Result<Self> {
        let mut history = Self::default();
        if workflow_ids.is_empty() {
            return Ok(history);
        }
        if snapshot.version != 1 {
            return Err("不支持的运行快照版本，无法归档".into());
        }
        let mut selected = BTreeSet::new();
        for id in workflow_ids {
            if id.is_empty() || !selected.insert(id) {
                return Err("归档流程 ID 为空或重复".into());
            }
            let mut matches = snapshot
                .workflows
                .iter()
                .filter(|workflow| &workflow.id == id);
            let workflow = matches
                .next()
                .ok_or_else(|| format!("归档流程不存在：{id}"))?;
            if matches.next().is_some() {
                return Err(format!("运行快照包含重复流程 ID：{id}").into());
            }
            history.workflows.push(ArchivedWorkflow::from(workflow));
        }
        history.validate()?;
        Ok(history)
    }

    /// Validate the archive schema and all structural references before use.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != HISTORY_SCHEMA_VERSION {
            return Err(format!("不支持的协作归档版本：{}", self.schema_version).into());
        }
        let mut workflow_ids = BTreeSet::new();
        let mut global_run_ids = BTreeSet::new();
        for workflow in &self.workflows {
            if workflow.id.is_empty() || !workflow_ids.insert(workflow.id.as_str()) {
                return Err("协作归档的流程 ID 为空或重复".into());
            }
            for plan in [&workflow.plan, &workflow.draft_plan].into_iter().flatten() {
                validate_plan(plan)?;
            }
            let task_ids: BTreeSet<_> = [&workflow.plan, &workflow.draft_plan]
                .into_iter()
                .flatten()
                .flat_map(|plan| &plan.tasks)
                .map(|task| task.id.as_str())
                .collect();
            let run_ids: BTreeSet<_> = workflow.runs.iter().map(|run| run.id.as_str()).collect();
            for run in &workflow.runs {
                if run.id.is_empty() || !global_run_ids.insert(run.id.as_str()) {
                    return Err("协作归档的运行 ID 为空或重复".into());
                }
                if !matches!(run.task_id.as_str(), "$plan" | "$test" | "$skill")
                    && !task_ids.contains(run.task_id.as_str())
                {
                    return Err(format!("运行引用了缺失的任务：{}", run.task_id).into());
                }
                let mut sources = BTreeSet::new();
                for source in &run.sources {
                    if source == &run.id
                        || !run_ids.contains(source.as_str())
                        || !sources.insert(source)
                    {
                        return Err(format!("运行的来源引用无效：{source}").into());
                    }
                }
                if let Some(previous) = &run.retry_of
                    && (previous == &run.id || !run_ids.contains(previous.as_str()))
                {
                    return Err(format!("运行的重试来源无效：{previous}").into());
                }
            }
        }
        Ok(())
    }
}

fn validate_plan(plan: &ArchivedPlan) -> Result<()> {
    let mut ids = BTreeSet::new();
    for task in &plan.tasks {
        if task.id.is_empty() || task.id.starts_with('$') || !ids.insert(task.id.as_str()) {
            return Err("协作归档的任务 ID 为空、重复或使用保留名称".into());
        }
    }
    for task in &plan.tasks {
        let mut dependencies = BTreeSet::new();
        for dependency in &task.dependencies {
            if dependency == &task.id
                || !ids.contains(dependency.as_str())
                || !dependencies.insert(dependency)
            {
                return Err(format!("任务的依赖引用无效：{dependency}").into());
            }
        }
        if let Some(parent) = &task.parent_agent
            && (parent == &task.id || !ids.contains(parent.as_str()))
        {
            return Err(format!("任务的父代理引用无效：{parent}").into());
        }
    }
    Ok(())
}

impl From<&collaboration::SourceFile> for ArchivedFile {
    fn from(file: &collaboration::SourceFile) -> Self {
        Self {
            path: file.path.clone(),
            content: file.content.clone(),
        }
    }
}

impl From<&collaboration::Plan> for ArchivedPlan {
    fn from(plan: &collaboration::Plan) -> Self {
        Self {
            tasks: plan
                .tasks
                .iter()
                .map(|task| ArchivedTask {
                    id: task.id.clone(),
                    title: task.title.clone(),
                    role: task.role.clone(),
                    instruction: task.instruction.clone(),
                    acceptance: task.acceptance.clone(),
                    dependencies: task.dependencies.clone(),
                    parent_agent: task.parent_agent.clone(),
                })
                .collect(),
        }
    }
}

impl From<&collaboration::Workflow> for ArchivedWorkflow {
    fn from(workflow: &collaboration::Workflow) -> Self {
        Self {
            id: workflow.id.clone(),
            goal: workflow.goal.clone(),
            model: workflow.model.clone(),
            files: workflow.files.iter().map(ArchivedFile::from).collect(),
            plan: workflow.plan.as_ref().map(ArchivedPlan::from),
            draft_plan: workflow.draft_plan.as_ref().map(ArchivedPlan::from),
            runs: workflow
                .runs
                .iter()
                .map(|run| ArchivedRun {
                    id: run.id.clone(),
                    task_id: run.task_id.clone(),
                    status: match run.status {
                        collaboration::Status::Queued => ArchivedStatus::Queued,
                        collaboration::Status::Running => ArchivedStatus::Running,
                        collaboration::Status::Stopping => ArchivedStatus::Stopping,
                        collaboration::Status::Succeeded => ArchivedStatus::Succeeded,
                        collaboration::Status::Failed => ArchivedStatus::Failed,
                        collaboration::Status::Cancelled => ArchivedStatus::Cancelled,
                        collaboration::Status::Unknown => ArchivedStatus::Unknown,
                        collaboration::Status::StorageBlocked => ArchivedStatus::StorageBlocked,
                    },
                    created_at: run.created_at,
                    finished_at: run.finished_at,
                    sources: run.sources.clone(),
                    prompt: run.prompt.clone(),
                    input_files: run.input_files.iter().map(ArchivedFile::from).collect(),
                    output: run.output.clone(),
                    proposal: run.proposal.as_ref().map(|proposal| ArchivedProposal {
                        summary: proposal.summary.clone(),
                        files: proposal.files.iter().map(ArchivedFile::from).collect(),
                    }),
                    error: run.error.clone(),
                    review: run.review.clone(),
                    usage: run.usage.clone(),
                    retry_of: run.retry_of.clone(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use collaboration::{Run, Snapshot, SourceFile, Status, Workflow};

    fn workflow(id: &str) -> Workflow {
        let files = vec![SourceFile {
            path: "src/main.rs".into(),
            content: "fn main() {}".into(),
        }];
        Workflow {
            id: id.into(),
            goal: format!("goal-{id}"),
            project: "/private/local/source-project".into(),
            model: "provider/model".into(),
            files: files.clone(),
            test_command: vec!["private-test-command".into()],
            plan: None,
            draft_plan: None,
            runs: vec![Run {
                id: format!("run-{id}"),
                task_id: "$plan".into(),
                status: Status::Running,
                created_at: 10,
                finished_at: None,
                sources: Vec::new(),
                prompt: "keep evidence /user/supplied/path".into(),
                input_files: files,
                output: "partial output".into(),
                proposal: None,
                error: None,
                review: None,
                usage: None,
                session_id: Some("private-session-handle".into()),
                retry_of: None,
            }],
            paused: false,
            max_calls: 16,
            timeout_seconds: 180,
        }
    }

    #[test]
    fn capture_is_an_explicit_allowlist_without_local_execution_bindings() {
        let snapshot = Snapshot {
            workflows: vec![workflow("selected"), workflow("unselected")],
            ..Snapshot::default()
        };
        let history = History::capture(&snapshot, &["selected".into()]).unwrap();
        let bytes = serde_json::to_vec(&history).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        for excluded in [
            "unselected",
            "/private/local/source-project",
            "private-session-handle",
            "private-test-command",
        ] {
            assert!(
                !text.contains(excluded),
                "unexpected archived value: {excluded}"
            );
        }
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for field in [
            "project",
            "paused",
            "maxCalls",
            "timeoutSeconds",
            "testCommand",
        ] {
            assert!(json["workflows"][0].get(field).is_none());
        }
        assert!(json["workflows"][0]["runs"][0].get("sessionId").is_none());
        assert_eq!(history.workflows[0].runs[0].status, ArchivedStatus::Running);
        assert!(text.contains("/user/supplied/path"));
        assert_eq!(serde_json::from_slice::<History>(&bytes).unwrap(), history);
        assert_eq!(snapshot.workflows[0].runs[0].status, Status::Running);
    }

    #[test]
    fn capture_rejects_missing_duplicate_and_ambiguous_selections() {
        let mut snapshot = Snapshot {
            workflows: vec![workflow("a")],
            ..Snapshot::default()
        };
        assert_eq!(
            History::capture(&snapshot, &[]).unwrap(),
            History::default()
        );
        assert!(History::capture(&snapshot, &["missing".into()]).is_err());
        assert!(History::capture(&snapshot, &["a".into(), "a".into()]).is_err());
        snapshot.workflows.push(workflow("a"));
        assert!(History::capture(&snapshot, &["a".into()]).is_err());
    }

    #[test]
    fn validation_rejects_unknown_schema_duplicate_ids_and_dangling_references() {
        let unknown: History =
            serde_json::from_str(r#"{"schemaVersion":2,"workflows":[]}"#).unwrap();
        assert!(unknown.validate().is_err());
        let snapshot = Snapshot {
            workflows: vec![workflow("a")],
            ..Snapshot::default()
        };
        let history = History::capture(&snapshot, &["a".into()]).unwrap();
        let mut invalid = history.clone();
        invalid.workflows.push(invalid.workflows[0].clone());
        assert!(invalid.validate().is_err());
        let mut invalid = history.clone();
        let duplicate = invalid.workflows[0].runs[0].clone();
        invalid.workflows[0].runs.push(duplicate);
        assert!(invalid.validate().is_err());
        let mut invalid = history.clone();
        invalid.workflows[0].runs[0].sources.push("missing".into());
        assert!(invalid.validate().is_err());
        let mut invalid = history.clone();
        invalid.workflows[0].runs[0].retry_of = Some("missing".into());
        assert!(invalid.validate().is_err());
        let mut invalid = history;
        invalid.workflows[0].runs[0].task_id = "missing".into();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn validation_accepts_complete_plan_and_run_provenance() {
        let mut source = workflow("a");
        source.plan = Some(collaboration::Plan {
            tasks: vec![collaboration::Task {
                id: "task-a".into(),
                title: "Implement".into(),
                role: "coder".into(),
                instruction: "Implement".into(),
                acceptance: "Tests pass".into(),
                dependencies: vec![],
                parent_agent: None,
            }],
        });
        let mut run = source.runs[0].clone();
        run.id = "implementation".into();
        run.task_id = "task-a".into();
        run.sources = vec![source.runs[0].id.clone()];
        source.runs.push(run.clone());
        run.id = "retry".into();
        run.retry_of = Some("implementation".into());
        source.runs.push(run);
        let history = History::capture(
            &Snapshot {
                workflows: vec![source],
                ..Snapshot::default()
            },
            &["a".into()],
        )
        .unwrap();
        history.validate().unwrap();
        let mut invalid = history;
        invalid.workflows[0].plan.as_mut().unwrap().tasks[0]
            .dependencies
            .push("missing".into());
        assert!(invalid.validate().is_err());
    }
}

//! Portable collaboration evidence, separate from the live execution store.
//!
//! These DTOs deliberately omit source-project bindings, session handles and
//! dispatch settings. Capturing or decoding them never starts or restores a Run.
//! Free-form evidence is preserved as supplied; this is not a content scrubber.
use crate::{Result, collaboration};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const HISTORY_ENTRY: &str = "collaboration/history.json";
pub const HISTORY_SCHEMA_VERSION: u32 = 1;
pub const DELEGATION_HISTORY_SCHEMA_VERSION: u32 = 2;
const MAX_GRAPH_NODES: usize = 65_536;
const MAX_GRAPH_EDGES: usize = 262_144;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct History<T = String> {
    pub schema_version: u32,
    pub workflows: Vec<ArchivedWorkflow<T>>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            schema_version: HISTORY_SCHEMA_VERSION,
            workflows: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedWorkflow<T = String> {
    pub id: String,
    pub goal: T,
    pub model: String,
    pub files: Vec<ArchivedFile<T>>,
    pub plan: Option<ArchivedPlan>,
    pub draft_plan: Option<ArchivedPlan>,
    pub runs: Vec<ArchivedRun<T>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedFile<T = String> {
    pub path: String,
    pub content: T,
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
pub struct ArchivedProposal<T = String> {
    pub summary: T,
    pub files: Vec<ArchivedFile<T>>,
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
pub struct ArchivedRun<T = String> {
    pub id: String,
    pub task_id: String,
    pub status: ArchivedStatus,
    pub created_at: u64,
    pub finished_at: Option<u64>,
    pub sources: Vec<String>,
    pub prompt: T,
    pub input_files: Vec<ArchivedFile<T>>,
    pub output: T,
    pub proposal: Option<ArchivedProposal<T>>,
    pub error: Option<T>,
    /// Historical evidence only; this does not authorize a local action.
    pub review: Option<String>,
    pub usage: Option<serde_json::Value>,
    pub retry_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<collaboration::Delegation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delegated_results: Vec<String>,
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
        if history
            .workflows
            .iter()
            .any(|w| w.runs.iter().any(|r| r.delegation.is_some()))
        {
            history.schema_version = DELEGATION_HISTORY_SCHEMA_VERSION;
        }
        history.validate()?;
        Ok(history)
    }
}

impl<T> History<T> {
    /// Transform body text without changing structural identities or metadata.
    /// The same DTO can therefore encode text or references to stored content.
    pub fn try_map_text<U>(self, mut map: impl FnMut(T) -> Result<U>) -> Result<History<U>> {
        let workflows = self
            .workflows
            .into_iter()
            .map(|workflow| {
                let goal = map(workflow.goal)?;
                let files = map_files(workflow.files, &mut map)?;
                let runs = workflow
                    .runs
                    .into_iter()
                    .map(|run| {
                        let prompt = map(run.prompt)?;
                        let input_files = map_files(run.input_files, &mut map)?;
                        let output = map(run.output)?;
                        let proposal = run
                            .proposal
                            .map(|proposal| {
                                Ok::<_, Box<dyn std::error::Error>>(ArchivedProposal {
                                    summary: map(proposal.summary)?,
                                    files: map_files(proposal.files, &mut map)?,
                                })
                            })
                            .transpose()?;
                        let error = run.error.map(&mut map).transpose()?;
                        Ok(ArchivedRun {
                            id: run.id,
                            task_id: run.task_id,
                            status: run.status,
                            created_at: run.created_at,
                            finished_at: run.finished_at,
                            sources: run.sources,
                            prompt,
                            input_files,
                            output,
                            proposal,
                            error,
                            review: run.review,
                            usage: run.usage,
                            retry_of: run.retry_of,
                            delegation: run.delegation,
                            delegated_results: run.delegated_results,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(ArchivedWorkflow {
                    id: workflow.id,
                    goal,
                    model: workflow.model,
                    files,
                    plan: workflow.plan,
                    draft_plan: workflow.draft_plan,
                    runs,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(History {
            schema_version: self.schema_version,
            workflows,
        })
    }

    /// Validate the archive schema and all structural references before use.
    pub fn validate(&self) -> Result<()> {
        if !matches!(
            self.schema_version,
            HISTORY_SCHEMA_VERSION | DELEGATION_HISTORY_SCHEMA_VERSION
        ) {
            return Err(format!("不支持的协作归档版本：{}", self.schema_version).into());
        }
        let mut budget = GraphBudget::default();
        budget.nodes(self.workflows.len())?;
        let mut workflow_ids = BTreeSet::new();
        let mut global_run_ids = BTreeSet::new();
        for workflow in &self.workflows {
            if workflow.id.is_empty() || !workflow_ids.insert(workflow.id.as_str()) {
                return Err("协作归档的流程 ID 为空或重复".into());
            }
            for plan in [&workflow.plan, &workflow.draft_plan].into_iter().flatten() {
                validate_plan(plan, &mut budget)?;
            }
            budget.nodes(workflow.runs.len())?;
            budget.edges(workflow.runs.iter().fold(0usize, |total, run| {
                total
                    .saturating_add(run.sources.len())
                    .saturating_add(usize::from(run.retry_of.is_some()))
                    .saturating_add(usize::from(run.delegation.is_some()))
                    .saturating_add(run.delegated_results.len())
            }))?;
            let task_ids: BTreeSet<_> = [&workflow.plan, &workflow.draft_plan]
                .into_iter()
                .flatten()
                .flat_map(|plan| &plan.tasks)
                .map(|task| task.id.as_str())
                .collect();
            let run_ids: BTreeSet<_> = workflow.runs.iter().map(|run| run.id.as_str()).collect();
            for run in &workflow.runs {
                if let Some(delegation) = &run.delegation
                    && (self.schema_version < 2
                        || delegation.parent_run_id == run.id
                        || !run_ids.contains(delegation.parent_run_id.as_str())
                        || delegation.call_id.is_empty()
                        || delegation.call_id.len() > 256
                        || delegation.request.len() > 32 * 1024)
                {
                    return Err("归档的委派来源或版本无效".into());
                }
                if run.id.is_empty() || !global_run_ids.insert(run.id.as_str()) {
                    return Err("协作归档的运行 ID 为空或重复".into());
                }
                if !matches!(run.task_id.as_str(), "$plan" | "$test" | "$skill")
                    && !task_ids.contains(run.task_id.as_str())
                {
                    return Err(format!("运行引用了缺失的任务：{}", run.task_id).into());
                }
                let mut sources = BTreeSet::new();
                for source in run.sources.iter().chain(&run.delegated_results) {
                    if source == &run.id
                        || !run_ids.contains(source.as_str())
                        || !sources.insert(source)
                    {
                        return Err(format!("运行的来源引用无效：{source}").into());
                    }
                }
                for result in &run.delegated_results {
                    if self.schema_version < 2
                        || !workflow.runs.iter().any(|r| {
                            &r.id == result
                                && r.delegation
                                    .as_ref()
                                    .is_some_and(|d| d.parent_run_id == run.id)
                        })
                    {
                        return Err("委派成果不属于本次父运行".into());
                    }
                }
                if let Some(previous) = &run.retry_of
                    && (previous == &run.id || !run_ids.contains(previous.as_str()))
                {
                    return Err(format!("运行的重试来源无效：{previous}").into());
                }
            }
            validate_acyclic(
                workflow.runs.iter().map(|run| run.id.as_str()),
                workflow.runs.iter().filter_map(|run| {
                    run.delegation
                        .as_ref()
                        .map(|d| (run.id.as_str(), d.parent_run_id.as_str()))
                }),
                "代理委派",
            )?;
            validate_acyclic(
                workflow.runs.iter().map(|run| run.id.as_str()),
                workflow.runs.iter().flat_map(|run| {
                    run.sources
                        .iter()
                        .chain(run.retry_of.iter())
                        .chain(run.delegated_results.iter())
                        .map(move |source| (run.id.as_str(), source.as_str()))
                }),
                "运行来源与重试",
            )?;
        }
        Ok(())
    }
}

fn map_files<T, U>(
    files: Vec<ArchivedFile<T>>,
    map: &mut impl FnMut(T) -> Result<U>,
) -> Result<Vec<ArchivedFile<U>>> {
    files
        .into_iter()
        .map(|file| {
            Ok(ArchivedFile {
                path: file.path,
                content: map(file.content)?,
            })
        })
        .collect()
}

fn validate_plan(plan: &ArchivedPlan, budget: &mut GraphBudget) -> Result<()> {
    budget.nodes(plan.tasks.len())?;
    budget.edges(plan.tasks.iter().fold(0usize, |total, task| {
        total
            .saturating_add(task.dependencies.len())
            .saturating_add(usize::from(task.parent_agent.is_some()))
    }))?;
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
    validate_acyclic(
        plan.tasks.iter().map(|task| task.id.as_str()),
        plan.tasks.iter().flat_map(|task| {
            task.dependencies
                .iter()
                .chain(task.parent_agent.iter())
                .map(move |parent| (task.id.as_str(), parent.as_str()))
        }),
        "任务依赖与父代理",
    )
}

struct GraphBudget {
    nodes_left: usize,
    edges_left: usize,
}

impl Default for GraphBudget {
    fn default() -> Self {
        Self {
            nodes_left: MAX_GRAPH_NODES,
            edges_left: MAX_GRAPH_EDGES,
        }
    }
}

impl GraphBudget {
    fn nodes(&mut self, count: usize) -> Result<()> {
        self.nodes_left = self
            .nodes_left
            .checked_sub(count)
            .ok_or("协作归档的结构对象过多")?;
        Ok(())
    }

    fn edges(&mut self, count: usize) -> Result<()> {
        self.edges_left = self
            .edges_left
            .checked_sub(count)
            .ok_or("协作归档的关系过多")?;
        Ok(())
    }
}

/// Kahn's algorithm uses an explicit work list, including for very deep chains.
/// Both reference kinds share a graph so mixed-kind cycles are rejected too.
fn validate_acyclic<'a>(
    ids: impl Iterator<Item = &'a str>,
    edges: impl Iterator<Item = (&'a str, &'a str)>,
    label: &str,
) -> Result<()> {
    let indices: BTreeMap<_, _> = ids.enumerate().map(|(index, id)| (id, index)).collect();
    let mut indegrees = vec![0usize; indices.len()];
    let mut followers = vec![Vec::new(); indices.len()];
    for (id, dependency) in edges {
        let &node = indices.get(id).ok_or("协作归档引用的对象不存在")?;
        let &parent = indices.get(dependency).ok_or("协作归档引用的对象不存在")?;
        indegrees[node] += 1;
        followers[parent].push(node);
    }
    let mut ready: Vec<_> = indegrees
        .iter()
        .enumerate()
        .filter_map(|(index, &degree)| (degree == 0).then_some(index))
        .collect();
    let mut visited = 0;
    while let Some(parent) = ready.pop() {
        visited += 1;
        for &node in &followers[parent] {
            indegrees[node] -= 1;
            if indegrees[node] == 0 {
                ready.push(node);
            }
        }
    }
    if visited != indices.len() {
        return Err(format!("协作归档的{label}存在循环").into());
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
                    delegation: run.delegation.clone(),
                    delegated_results: run.delegated_results.clone(),
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
                delegation: None,
                delegated_results: vec![],
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
            serde_json::from_str(r#"{"schemaVersion":3,"workflows":[]}"#).unwrap();
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

    #[test]
    fn body_mapping_round_trips_all_text_and_preserves_metadata() {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct TextRef {
            object: usize,
        }

        let mut source = workflow("mapping");
        source.goal = "共享正文".into();
        source.files[0].content = "共享正文".into();
        source.runs[0].error = Some("错误正文".into());
        source.runs[0].review = Some("approved".into());
        source.runs[0].usage = Some(serde_json::json!({"input": 42}));
        source.runs[0].proposal = Some(collaboration::Proposal {
            summary: "总结正文".into(),
            files: vec![SourceFile {
                path: "generated.rs".into(),
                content: "成果正文".into(),
            }],
        });
        let mut history = History::capture(
            &Snapshot {
                workflows: vec![source],
                ..Snapshot::default()
            },
            &["mapping".into()],
        )
        .unwrap();
        history.workflows[0].plan = Some(ArchivedPlan {
            tasks: vec![archived_task("task")],
        });
        history.workflows[0].draft_plan = history.workflows[0].plan.clone();
        let mut objects: Vec<String> = Vec::new();
        let mapped = history
            .clone()
            .try_map_text(|text| {
                let object = if let Some(index) = objects.iter().position(|saved| saved == &text) {
                    index
                } else {
                    objects.push(text);
                    objects.len() - 1
                };
                Ok(TextRef { object })
            })
            .unwrap();
        mapped.validate().unwrap();
        assert_eq!(
            mapped.workflows[0].goal,
            mapped.workflows[0].files[0].content
        );
        let json = serde_json::to_value(&mapped).unwrap();
        let run = &json["workflows"][0]["runs"][0];
        for body in [
            &json["workflows"][0]["goal"],
            &json["workflows"][0]["files"][0]["content"],
            &run["prompt"],
            &run["inputFiles"][0]["content"],
            &run["output"],
            &run["error"],
            &run["proposal"]["summary"],
            &run["proposal"]["files"][0]["content"],
        ] {
            assert!(
                body.get("object")
                    .and_then(serde_json::Value::as_u64)
                    .is_some()
            );
        }
        assert_eq!(run["review"], "approved");
        assert_eq!(run["usage"], serde_json::json!({"input": 42}));
        assert_eq!(
            json["workflows"][0]["plan"]["tasks"][0]["instruction"],
            "instruction"
        );
        let decoded: History<TextRef> = serde_json::from_value(json).unwrap();
        let restored = decoded
            .try_map_text(|reference| {
                objects
                    .get(reference.object)
                    .cloned()
                    .ok_or_else(|| "正文对象不存在".into())
            })
            .unwrap();
        assert_eq!(restored, history);
        assert!(
            history
                .try_map_text::<usize>(|_| Err("正文存储失败".into()))
                .is_err()
        );
    }

    fn archived_task(id: &str) -> ArchivedTask {
        ArchivedTask {
            id: id.into(),
            title: "title".into(),
            role: "role".into(),
            instruction: "instruction".into(),
            acceptance: "acceptance".into(),
            dependencies: vec![],
            parent_agent: None,
        }
    }

    fn history_with_runs(count: usize) -> History {
        let mut history = History::capture(
            &Snapshot {
                workflows: vec![workflow("graph")],
                ..Snapshot::default()
            },
            &["graph".into()],
        )
        .unwrap();
        let mut template = history.workflows[0].runs[0].clone();
        template.input_files.clear();
        history.workflows[0].runs = (0..count)
            .map(|index| {
                let mut run = template.clone();
                run.id = index.to_string();
                run
            })
            .collect();
        history
    }

    #[test]
    fn validation_rejects_multi_run_source_retry_and_mixed_cycles() {
        for kind in ["sources", "retry", "mixed"] {
            let mut history = history_with_runs(3);
            for (index, run) in history.workflows[0].runs.iter_mut().enumerate() {
                let reference = ((index + 1) % 3).to_string();
                if kind == "retry" || (kind == "mixed" && index == 1) {
                    run.retry_of = Some(reference);
                } else {
                    run.sources.push(reference);
                }
            }
            let error = history.validate().unwrap_err().to_string();
            assert!(error.contains("循环"), "{kind}: {error}");
        }
    }

    #[test]
    fn validation_rejects_multi_task_dependency_parent_and_mixed_cycles() {
        for kind in ["dependencies", "parent", "mixed"] {
            let mut history = history_with_runs(1);
            let tasks = (0..3)
                .map(|index| {
                    let mut task = archived_task(&index.to_string());
                    let reference = ((index + 1) % 3).to_string();
                    if kind == "parent" || (kind == "mixed" && index == 1) {
                        task.parent_agent = Some(reference);
                    } else {
                        task.dependencies.push(reference);
                    }
                    task
                })
                .collect();
            history.workflows[0].draft_plan = Some(ArchivedPlan { tasks });
            let error = history.validate().unwrap_err().to_string();
            assert!(error.contains("循环"), "{kind}: {error}");
        }
    }

    #[test]
    fn validation_handles_deep_chains_iteratively_and_limits_graph_work() {
        let mut history = history_with_runs(20_000);
        for (index, run) in history.workflows[0].runs.iter_mut().enumerate().skip(1) {
            run.sources.push((index - 1).to_string());
        }
        history.validate().unwrap();
        history.workflows[0].runs[0].sources.push("19999".into());
        assert!(history.validate().unwrap_err().to_string().contains("循环"));

        let mut oversized = history_with_runs(1);
        oversized.workflows[0].runs[0].sources = vec!["missing".into(); MAX_GRAPH_EDGES + 1];
        assert!(
            oversized
                .validate()
                .unwrap_err()
                .to_string()
                .contains("关系过多")
        );

        let oversized: History = History {
            schema_version: HISTORY_SCHEMA_VERSION,
            workflows: vec![
                ArchivedWorkflow {
                    id: "unused".into(),
                    goal: String::new(),
                    model: String::new(),
                    files: vec![],
                    plan: None,
                    draft_plan: None,
                    runs: vec![],
                };
                MAX_GRAPH_NODES + 1
            ],
        };
        assert!(
            oversized
                .validate()
                .unwrap_err()
                .to_string()
                .contains("对象过多")
        );
    }
}

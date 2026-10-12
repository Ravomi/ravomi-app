//! Canvas bindings and selected evidence travel with the document; execution stays local.
use super::*;
use crate::document::Document;
use std::collections::HashSet;
mod mind_map;
mod network;

const ATTACHMENT: &str = "ravomi-code-agents.json";
pub const AGENT_CARD_SIZE: [f32; 2] = [360.0, 180.0];

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Bindings {
    pub workflows: Vec<Binding>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Binding {
    pub workflow_id: String,
    pub input_ids: Vec<String>,
    pub anchor: [f32; 2],
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub answer_maps: Vec<AnswerMap>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct AnswerMap {
    pub run_id: String,
    pub node_ids: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Agent {
    pub task_id: String,
    pub group_id: String,
    pub log_id: String,
    #[serde(default)]
    pub presented_run_id: Option<String>,
}

impl Bindings {
    pub fn is_log(&self, id: &str) -> bool {
        self.workflows
            .iter()
            .flat_map(|w| &w.agents)
            .any(|a| a.log_id == id)
    }
    pub fn read(doc: &Document) -> Result<Self> {
        doc.attachments
            .get(ATTACHMENT)
            .map(|bytes| {
                serde_json::from_slice(bytes).map_err(|e| format!("画布 AI 绑定损坏，未覆盖：{e}"))
            })
            .unwrap_or_else(|| Ok(Self::default()))
    }
    pub fn write(&self, doc: &mut Document) -> Result<()> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if doc.attachments.get(ATTACHMENT) != Some(&bytes) {
            doc.attachments.insert(ATTACHMENT.into(), bytes);
            doc.mark_project_changed();
        }
        Ok(())
    }
    fn generated(&self) -> HashSet<&str> {
        self.workflows
            .iter()
            .flat_map(|w| {
                w.agents
                    .iter()
                    .flat_map(|a| [a.group_id.as_str(), a.log_id.as_str()])
                    .chain(
                        w.answer_maps
                            .iter()
                            .flat_map(|map| map.node_ids.iter().map(String::as_str)),
                    )
            })
            .collect()
    }
    pub fn capture(
        &self,
        doc: &Document,
        selected: &HashSet<usize>,
    ) -> Result<(String, Vec<String>, [f32; 2])> {
        let generated = self.generated();
        let mut included = BTreeSet::new();
        let mut todo: Vec<_> = selected.iter().copied().collect();
        while let Some(i) = todo.pop() {
            let Some(node) = doc.nodes.get(i) else {
                continue;
            };
            if generated.contains(node.id.as_str()) || !included.insert(i) {
                continue;
            }
            todo.extend(
                node.children
                    .iter()
                    .filter_map(|id| doc.nodes.iter().position(|n| &n.id == id)),
            );
        }
        let mut ids = vec![];
        let mut text = vec![];
        let mut anchor = [0.0_f32, 0.0_f32];
        for i in included {
            let n = &doc.nodes[i];
            if !matches!(n.kind.as_str(), "TextNode" | "Section") || n.text.trim().is_empty() {
                continue;
            }
            if ids.is_empty() {
                anchor[1] = n.rect[1];
            }
            anchor[0] = anchor[0].max(n.rect[0] + n.rect[2] + 160.0);
            ids.push(n.id.clone());
            text.push(n.text.clone());
        }
        let goal = text.join("\n\n");
        if goal.trim().is_empty() {
            return Err("请选择有内容的文本块或分组块；Agent 输出不会自动作为新任务提交".into());
        }
        if goal.len() > 16_000 {
            return Err("选中文字超过 16000 字节，请缩小范围".into());
        }
        Ok((goal, ids, anchor))
    }
    pub fn selected_runs(
        &self,
        doc: &Document,
        selected: &HashSet<usize>,
        snapshot: &Snapshot,
    ) -> Vec<skills::RunRef> {
        let selected = doc.expand_selection(selected);
        let ids: HashSet<_> = selected
            .iter()
            .filter_map(|&i| doc.nodes.get(i).map(|n| n.id.as_str()))
            .collect();
        self.workflows
            .iter()
            .flat_map(|b| {
                let ids = &ids;
                snapshot
                    .workflows
                    .iter()
                    .filter(move |w| w.id == b.workflow_id)
                    .flat_map(move |w| {
                        w.runs
                            .iter()
                            .filter(|r| {
                                b.answer_maps.iter().any(|map| {
                                    map.run_id == r.id
                                        && map.node_ids.iter().any(|id| ids.contains(id.as_str()))
                                }) || b.agents.iter().any(|a| {
                                    a.task_id == r.task_id
                                        && (ids.contains(a.group_id.as_str())
                                            || ids.contains(a.log_id.as_str()))
                                })
                            })
                            .map(|r| skills::RunRef {
                                workflow_id: w.id.clone(),
                                run_id: r.id.clone(),
                            })
                            .collect::<Vec<_>>()
                    })
            })
            .collect()
    }
    /// Never recreates a deleted agent node and never modifies the user's input blocks.
    pub fn sync(&mut self, doc: &mut Document, snapshot: &Snapshot) -> Result<()> {
        if doc.in_transaction() {
            return Ok(());
        }
        let creates_nodes = self.workflows.iter().any(|binding| {
            snapshot
                .workflows
                .iter()
                .find(|workflow| workflow.id == binding.workflow_id)
                .is_some_and(|workflow| {
                    !mind_map::eligible(workflow, binding)
                        && tasks(workflow).iter().any(|(id, _, _)| {
                            !binding.agents.iter().any(|agent| &agent.task_id == id)
                        })
                })
        });
        let creates_maps = self.workflows.iter().any(|binding| {
            snapshot.workflows.iter().any(|w| {
                w.id == binding.workflow_id
                    && mind_map::eligible(w, binding)
                    && w.runs.iter().any(|run| {
                        run.status == Status::Succeeded
                            && run.proposal.is_some()
                            && !binding.answer_maps.iter().any(|map| map.run_id == run.id)
                    })
            })
        });
        if creates_nodes || creates_maps {
            // Creation can fail on locked groups or invalid geometry. Validate the
            // entire projection on a detached copy before changing saved evidence.
            let mut staged = doc.capture_snapshot().map_err(|error| error.to_string())?;
            self.clone().sync_inner(&mut staged, snapshot)?;
        }
        self.sync_inner(doc, snapshot)
    }

    fn sync_inner(&mut self, doc: &mut Document, snapshot: &Snapshot) -> Result<()> {
        // The caller supplies only workflows explicitly connected to this document
        // instance. Missing local workflows never remove previously saved evidence.
        let ids: Vec<_> = self
            .workflows
            .iter()
            .filter(|binding| {
                snapshot
                    .workflows
                    .iter()
                    .any(|workflow| workflow.id == binding.workflow_id)
            })
            .map(|binding| binding.workflow_id.clone())
            .collect();
        let history = crate::project_history::History::capture(snapshot, &ids)
            .map_err(|error| error.to_string())?;
        doc.attach_history(history)
            .map_err(|error| error.to_string())?;
        let mut changed_text = false;
        for binding in &mut self.workflows {
            let Some(w) = snapshot
                .workflows
                .iter()
                .find(|w| w.id == binding.workflow_id)
            else {
                continue;
            };
            if mind_map::eligible(w, binding) {
                mind_map::sync(doc, w, binding)?;
                continue;
            }
            for (task_id, title, parent) in tasks(w) {
                if !binding.agents.iter().any(|a| a.task_id == task_id) {
                    let depth = w.plan.as_ref().map_or(1, |p| p.tasks.len());
                    let column = layer(w, &task_id, depth);
                    let row = binding
                        .agents
                        .iter()
                        .filter(|a| layer(w, &a.task_id, depth) == column)
                        .count();
                    let log = doc
                        .create_text_node([
                            binding.anchor[0] + column as f32 * 520.0,
                            binding.anchor[1] + row as f32 * 360.0,
                        ])
                        .map_err(|e| e.to_string())?;
                    doc.graph.nodes[log].auto_size = false;
                    doc.graph.nodes[log].font_size = 18.0;
                    doc.graph.nodes[log].rect[2..4].copy_from_slice(&[400.0, 240.0]);
                    let group = doc
                        .create_group(&HashSet::from([log]))
                        .map_err(|e| e.to_string())?;
                    doc.graph.nodes[group].font_size = 22.0;
                    doc.set_sections_collapsed(&HashSet::from([group]), true)
                        .map_err(|e| e.to_string())?;
                    binding.agents.push(Agent {
                        task_id: task_id.clone(),
                        group_id: doc.nodes[group].id.clone(),
                        log_id: doc.nodes[log].id.clone(),
                        presented_run_id: None,
                    });
                    let upstream: Vec<_> = if task_id == "$plan" {
                        binding
                            .input_ids
                            .iter()
                            .filter(|id| {
                                !binding.input_ids.iter().any(|parent| {
                                    doc.nodes
                                        .iter()
                                        .any(|n| &n.id == parent && n.children.contains(id))
                                })
                            })
                            .cloned()
                            .collect()
                    } else {
                        let dependencies = w
                            .plan
                            .as_ref()
                            .or(w.draft_plan.as_ref())
                            .and_then(|p| p.tasks.iter().find(|t| t.id == task_id))
                            .map(|t| {
                                let mut upstream = t.dependencies.clone();
                                if let Some(parent) = &t.parent_agent
                                    && !upstream.contains(parent)
                                {
                                    upstream.push(parent.clone());
                                }
                                upstream
                            })
                            .unwrap_or_default();
                        binding
                            .agents
                            .iter()
                            .filter(|a| {
                                dependencies.contains(&a.task_id)
                                    || (dependencies.is_empty() && a.task_id == "$plan")
                            })
                            .map(|a| a.group_id.clone())
                            .collect()
                    };
                    for source in upstream {
                        if let Some(index) = doc.nodes.iter().position(|n| n.id == source) {
                            doc.add_edge(crate::GraphEdge {
                                id: id(),
                                members: vec![index, group],
                                rates: vec![[1.0, 0.5], [0.0, 0.5]],
                                text: if binding.agents.iter().any(|a| {
                                    a.group_id == source && parent.as_ref() == Some(&a.task_id)
                                }) {
                                    "委派".into()
                                } else {
                                    String::new()
                                },
                                ..Default::default()
                            })
                            .map_err(|e| e.to_string())?;
                        }
                    }
                }
                let agent = binding
                    .agents
                    .iter_mut()
                    .find(|a| a.task_id == task_id)
                    .unwrap();
                let runs: Vec<_> = w.runs.iter().filter(|r| r.task_id == task_id).collect();
                let state = runs.last().map(|r| status(&r.status)).unwrap_or("等待依赖");
                let heading = format!("{title} · {state}");
                let latest = runs.last().copied();
                let body = response_body(w, &task_id, latest, parent.as_deref());
                // Completing a run never unfolds a large log over nearby work.
                if let Some(run) = latest.filter(|r| r.status == Status::Succeeded)
                    && agent.presented_run_id.as_deref() != Some(&run.id)
                {
                    agent.presented_run_id = Some(run.id.clone());
                }
                for (id, text) in [(&agent.group_id, heading), (&agent.log_id, body)] {
                    if let Some(node) = doc.graph.nodes.iter_mut().find(|n| &n.id == id)
                        && node.text != text
                    {
                        node.text = text;
                        changed_text = true;
                    }
                }
            }
        }
        if changed_text {
            doc.mark_project_changed();
        }
        self.write(doc)
    }
}

fn response_body(w: &Workflow, task_id: &str, run: Option<&Run>, parent: Option<&str>) -> String {
    if let Some(run) = run {
        if let Some(proposal) = &run.proposal {
            return network::summary_text(&proposal.summary)
                .unwrap_or_else(|| proposal.summary.clone());
        }
        if let Some(error) = &run.error {
            return format!("任务未完成。\n{error}");
        }
        if task_id == "$plan"
            && let Some(plan) = w.plan.as_ref().or(w.draft_plan.as_ref())
        {
            return plan
                .tasks
                .iter()
                .enumerate()
                .map(|(i, task)| {
                    format!(
                        "{}. {}\n{}\n完成条件：{}",
                        i + 1,
                        task.title,
                        task.instruction,
                        task.acceptance
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");
        }
        return if run.status == Status::Succeeded {
            "本次运行没有可显示的正文。请查看运行详情。".into()
        } else {
            format!("{}。完成后在此显示正文。", status(&run.status))
        };
    }
    parent.map_or_else(
        || "等待开始。".into(),
        |parent| format!("等待上游任务 {parent}。"),
    )
}

fn tasks(workflow: &Workflow) -> Vec<(String, String, Option<String>)> {
    if workflow.runs.iter().any(|run| run.task_id == "$skill") {
        return vec![("$skill".into(), format!("Skill · {}", workflow.goal), None)];
    }
    let mut tasks = vec![("$plan".into(), "任务规划".into(), None)];
    if let Some(plan) = workflow.plan.as_ref().or(workflow.draft_plan.as_ref()) {
        tasks.extend(plan.tasks.iter().map(|task| {
            (
                task.id.clone(),
                task.title.clone(),
                task.parent_agent.clone(),
            )
        }));
    }
    tasks
}

fn layer(workflow: &Workflow, task_id: &str, remaining: usize) -> usize {
    if task_id.starts_with('$') || remaining == 0 {
        return 0;
    }
    workflow
        .plan
        .as_ref()
        .or(workflow.draft_plan.as_ref())
        .and_then(|p| p.tasks.iter().find(|t| t.id == task_id))
        .map(|t| {
            1 + t
                .dependencies
                .iter()
                .chain(t.parent_agent.iter())
                .map(|id| layer(workflow, id, remaining - 1))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(1)
}

pub fn status(status: &Status) -> &'static str {
    match status {
        Status::Queued => "排队",
        Status::Running => "执行中",
        Status::Stopping => "停止中",
        Status::Succeeded => "已完成",
        Status::Failed => "失败",
        Status::Cancelled => "已取消",
        Status::Unknown => "待核对",
        Status::StorageBlocked => "保存受阻",
    }
}

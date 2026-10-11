//! Canvas bindings travel with the document; execution records remain in the local runtime.
use super::*;
use crate::document::Document;
use std::collections::HashSet;

const ATTACHMENT: &str = "ravomi-code-agents.json";

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
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Agent {
    pub task_id: String,
    pub group_id: String,
    pub log_id: String,
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
            doc.revision += 1;
        }
        Ok(())
    }
    fn generated(&self) -> HashSet<&str> {
        self.workflows
            .iter()
            .flat_map(|w| &w.agents)
            .flat_map(|a| [a.group_id.as_str(), a.log_id.as_str()])
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
                                b.agents.iter().any(|a| {
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
        for binding in &mut self.workflows {
            let Some(w) = snapshot
                .workflows
                .iter()
                .find(|w| w.id == binding.workflow_id)
            else {
                continue;
            };
            let mut tasks = vec![("$plan".to_string(), "任务规划".to_string(), None)];
            if w.runs.iter().any(|r| r.task_id == "$skill") {
                tasks = vec![("$skill".into(), format!("Skill · {}", w.goal), None)];
            } else if let Some(p) = w.plan.as_ref().or(w.draft_plan.as_ref()) {
                tasks.extend(
                    p.tasks
                        .iter()
                        .map(|t| (t.id.clone(), t.title.clone(), t.parent_agent.clone())),
                );
            }
            for (task_id, title, parent) in tasks {
                if !binding.agents.iter().any(|a| a.task_id == task_id) {
                    let column = layer(w, &task_id, MAX_TASKS);
                    let row = binding
                        .agents
                        .iter()
                        .filter(|a| layer(w, &a.task_id, MAX_TASKS) == column)
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
                            .map(|t| t.dependencies.clone())
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
                                text: String::new(),
                                ..Default::default()
                            })
                            .map_err(|e| e.to_string())?;
                        }
                    }
                }
                let agent = binding
                    .agents
                    .iter()
                    .find(|a| a.task_id == task_id)
                    .unwrap();
                let runs: Vec<_> = w.runs.iter().filter(|r| r.task_id == task_id).collect();
                let state = runs.last().map(|r| status(&r.status)).unwrap_or("等待依赖");
                let heading = format!("{title}\n{state}");
                let mut body = format!(
                    "{title}\n流程：{}\n父代理：{}\n",
                    w.id,
                    parent.as_deref().unwrap_or("无")
                );
                for run in runs {
                    body.push_str(&format!(
                        "\n运行：{} · {}\n{}\n",
                        run.id,
                        status(&run.status),
                        run.error.as_deref().unwrap_or("")
                    ));
                    body.extend(run.output.chars().take(2500));
                    if run.output.chars().count() > 2500 {
                        body.push_str("\n…完整记录见 AI 详情");
                    }
                }
                for (id, text) in [(&agent.group_id, heading), (&agent.log_id, body)] {
                    if let Some(node) = doc.graph.nodes.iter_mut().find(|n| &n.id == id)
                        && node.text != text
                    {
                        node.text = text;
                        doc.revision += 1;
                    }
                }
            }
        }
        self.write(doc)
    }
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

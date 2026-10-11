//! Answers are editable text trees; planning and execution evidence stay archived.
use super::*;

pub(super) fn eligible(w: &Workflow, binding: &Binding) -> bool {
    w.project.is_empty()
        && binding.agents.is_empty()
        && !w.runs.iter().any(|run| run.task_id == "$skill")
        && !w.runs.iter().any(|run| run.delegation.is_some())
}

struct Topic {
    text: String,
    parent: Option<usize>,
    depth: usize,
    height: f32,
    span: f32,
}

fn topics(summary: &str) -> Result<Vec<Topic>> {
    let mut topics: Vec<Topic> = vec![];
    let mut stack: Vec<usize> = vec![];
    for line in summary.lines().filter(|line| !line.trim().is_empty()) {
        let trimmed = line.trim_start();
        let bullet = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "));
        let depth = if bullet.is_some() {
            (line.len() - trimmed.len()) / 2
        } else {
            0
        };
        if depth > 8 || topics.len() >= 256 {
            return Err("回答导图超过 8 层或 256 个节点，请缩小本次问题范围".into());
        }
        let depth = depth.min(stack.len());
        stack.truncate(depth);
        let text = bullet.unwrap_or(trimmed).trim().to_owned();
        if text.is_empty() {
            continue;
        }
        // Conservative CJK estimate reserves sibling space until font layout completes.
        let height = ((text.chars().count().max(1).div_ceil(14) as f32) * 28.0 + 20.0).max(48.0);
        topics.push(Topic {
            text,
            parent: stack.last().copied(),
            depth,
            height,
            span: height,
        });
        stack.push(topics.len() - 1);
    }
    for i in (0..topics.len()).rev() {
        let children: f32 = topics
            .iter()
            .filter(|t| t.parent == Some(i))
            .map(|t| t.span + 32.0)
            .sum();
        topics[i].span = topics[i].height.max((children - 32.0).max(0.0));
    }
    if topics.is_empty() {
        return Err("回答导图缺少正文".into());
    }
    Ok(topics)
}

pub(super) fn sync(doc: &mut Document, w: &Workflow, binding: &mut Binding) -> Result<()> {
    for run in w
        .runs
        .iter()
        .filter(|run| run.status == Status::Succeeded && run.proposal.is_some())
    {
        if binding.answer_maps.iter().any(|map| map.run_id == run.id) {
            continue;
        }
        let summary = &run.proposal.as_ref().unwrap().summary;
        if summary.trim_start().starts_with('{') {
            network::project(doc, binding, &run.id, summary)?;
            continue;
        }
        let outline = topics(summary)?;
        let sources: Vec<_> = binding
            .input_ids
            .iter()
            .filter_map(|id| doc.nodes.iter().position(|n| &n.id == id))
            .filter(|&index| {
                !binding.input_ids.iter().any(|id| {
                    doc.nodes
                        .iter()
                        .any(|n| &n.id == id && n.children.contains(&doc.nodes[index].id))
                })
            })
            .collect();
        // A deleted input is not an invitation to reconstruct user content.
        if sources.is_empty() && !binding.input_ids.is_empty() {
            continue;
        }
        let span: f32 = outline
            .iter()
            .filter(|topic| topic.parent.is_none())
            .map(|topic| topic.span + 48.0)
            .sum();
        let first_top = binding.anchor[1] - (span - 48.0).max(0.0) * 0.5;
        let mut y = binding
            .answer_maps
            .iter()
            .flat_map(|map| &map.node_ids)
            .filter_map(|id| doc.nodes.iter().find(|n| &n.id == id))
            .map(|n| n.rect[1] + n.rect[3] + 64.0)
            .fold(first_top, f32::max);
        let mut indices = Vec::new();
        let mut tops = Vec::new();
        let mut offsets = vec![0.0; outline.len()];
        for (i, topic) in outline.iter().enumerate() {
            let top = if let Some(parent) = topic.parent {
                let top = tops[parent] + offsets[parent];
                offsets[parent] += topic.span + 32.0;
                top
            } else {
                let top = y;
                y += topic.span + 48.0;
                top
            };
            tops.push(top);
            let x = binding.anchor[0] + topic.depth as f32 * 360.0;
            let y = top + (topic.span - topic.height) * 0.5;
            let index = doc
                .create_text_node([x + 140.0, y + topic.height * 0.5])
                .map_err(|e| e.to_string())?;
            let node = &mut doc.graph.nodes[index];
            node.text.clone_from(&topic.text);
            node.font_size = 18.0;
            node.auto_size = false;
            node.rect = [x, y, 280.0, topic.height];
            let parents = topic
                .parent
                .map_or_else(|| sources.clone(), |parent| vec![indices[parent]]);
            for parent in parents {
                doc.add_edge(crate::GraphEdge {
                    id: id(),
                    members: vec![parent, index],
                    rates: vec![[1.0, 0.5], [0.0, 0.5]],
                    ..Default::default()
                })
                .map_err(|e| e.to_string())?;
            }
            indices.push(index);
            debug_assert_eq!(indices.len(), i + 1);
        }
        binding.answer_maps.push(AnswerMap {
            run_id: run.id.clone(),
            node_ids: indices.iter().map(|&i| doc.nodes[i].id.clone()).collect(),
        });
        doc.mark_project_changed();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_git_example_projects_as_a_four_level_tree() {
        let summary = include_str!("../../../validation/fixtures/git-outline.md");
        let outline = topics(summary).unwrap();
        assert_eq!(outline.iter().filter(|t| t.parent.is_none()).count(), 5);
        assert_eq!(outline.iter().map(|t| t.depth).max(), Some(2));
        let workflow: Workflow = serde_json::from_value(serde_json::json!({
            "id":"example", "goal":"Git", "project":"", "model":"test/fixture",
            "files":[], "testCommand":[], "plan":null, "draftPlan":null,
            "paused":true, "maxCalls":0, "timeoutSeconds":60,
            "runs":[{"id":"answer-run", "taskId":"answer", "status":"succeeded",
                "createdAt":1, "sources":[], "prompt":"fixture", "inputFiles":[],
                "output":"", "proposal":{"summary":summary,"files":[]}}]
        }))
        .unwrap();
        let mut doc = Document::blank().unwrap();
        let root = doc.create_text_node([0.0, 0.0]).unwrap();
        doc.graph.nodes[root].text = "Git".into();
        let mut binding = Binding {
            workflow_id: workflow.id.clone(),
            input_ids: vec![doc.nodes[root].id.clone()],
            anchor: [240.0, 0.0],
            agents: vec![],
            answer_maps: vec![],
        };
        sync(&mut doc, &workflow, &mut binding).unwrap();
        assert_eq!(doc.nodes.len(), outline.len() + 1);
        assert_eq!(doc.edges.len(), outline.len());
        assert_eq!(doc.edges.iter().filter(|e| e.members[0] == root).count(), 5);
        for (index, topic) in outline.iter().enumerate() {
            assert_eq!(doc.nodes[index + 1].text, topic.text);
            let parent = topic.parent.map_or(root, |p| p + 1);
            assert!(doc.edges.iter().any(|e| e.members == [parent, index + 1]));
        }
        if let Some(path) = std::env::var_os("RAVOMI_GIT_TREE_FIXTURE") {
            doc.save(std::path::Path::new(&path)).unwrap();
        }
    }
    #[test]
    fn outline_preserves_hierarchy_and_reserves_subtree_space() {
        let outline =
            topics("- 定义\n  - 分布式版本控制\n- 用途\n  - 保存历史\n  - 比较改动").unwrap();
        assert_eq!(outline.len(), 5);
        assert_eq!(outline[1].parent, Some(0));
        assert_eq!(outline[3].parent, Some(2));
        assert_eq!(outline[4].parent, Some(2));
        assert!(outline[2].span >= outline[3].height + outline[4].height + 32.0);
        assert_eq!(outline[1].text, "分布式版本控制");
    }
}

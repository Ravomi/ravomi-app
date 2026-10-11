//! Containment and knowledge relations are independent: groups nest, edges can form a network.
use super::*;
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Network {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    id: String,
    text: String,
    #[serde(default)]
    parent: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    from: String,
    to: String,
    label: String,
}

impl Network {
    fn parse(summary: &str) -> Result<Self> {
        let graph: Self =
            serde_json::from_str(summary).map_err(|e| format!("回答关系网格式无效：{e}"))?;
        if graph.nodes.is_empty() || graph.nodes.len() > 256 || graph.edges.len() > 1024 {
            return Err("回答关系网需要 1 至 256 个节点，最多 1024 条关系".into());
        }
        let mut ids = HashMap::new();
        for (i, node) in graph.nodes.iter().enumerate() {
            if node.id.trim().is_empty()
                || node.text.trim().is_empty()
                || ids.insert(node.id.as_str(), i).is_some()
            {
                return Err("回答关系网节点 ID 重复或正文为空".into());
            }
        }
        for node in &graph.nodes {
            let mut seen = HashSet::from([node.id.as_str()]);
            let mut parent = node.parent.as_deref();
            while let Some(id) = parent {
                if !seen.insert(id) || seen.len() > 9 {
                    return Err("回答分组存在循环或超过 8 层嵌套".into());
                }
                let index = ids.get(id).ok_or("回答分组引用不存在的节点")?;
                parent = graph.nodes[*index].parent.as_deref();
            }
        }
        for edge in &graph.edges {
            if !ids.contains_key(edge.from.as_str())
                || !ids.contains_key(edge.to.as_str())
                || edge.label.trim().is_empty()
            {
                return Err("回答关系缺少有效端点或关系名称".into());
            }
        }
        Ok(graph)
    }

    fn children(&self, parent: Option<&str>) -> Vec<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| (n.parent.as_deref() == parent).then_some(i))
            .collect()
    }

    fn size(&self, i: usize) -> [f32; 2] {
        let children = self.children(Some(&self.nodes[i].id));
        if children.is_empty() {
            return [
                280.0,
                (self.nodes[i].text.chars().count().max(1).div_ceil(14) as f32 * 28.0 + 20.0)
                    .max(64.0),
            ];
        }
        let sizes: Vec<_> = children.iter().map(|&c| self.size(c)).collect();
        [
            sizes.iter().map(|s| s[0]).fold(280.0, f32::max) + 64.0,
            sizes.iter().map(|s| s[1] + 32.0).sum::<f32>() + 64.0,
        ]
    }

    fn create(
        &self,
        doc: &mut Document,
        i: usize,
        at: [f32; 2],
        indices: &mut [usize],
    ) -> Result<usize> {
        let children = self.children(Some(&self.nodes[i].id));
        let size = self.size(i);
        let index = if children.is_empty() {
            doc.create_text_node([at[0] + size[0] * 0.5, at[1] + size[1] * 0.5])
                .map_err(|e| e.to_string())?
        } else {
            let mut y = at[1] + 64.0;
            let mut members = HashSet::new();
            for child in children {
                members.insert(self.create(doc, child, [at[0] + 32.0, y], indices)?);
                y += self.size(child)[1] + 32.0;
            }
            doc.create_group(&members).map_err(|e| e.to_string())?
        };
        let node = &mut doc.graph.nodes[index];
        node.text.clone_from(&self.nodes[i].text);
        node.font_size = if node.children.is_empty() { 18.0 } else { 24.0 };
        node.auto_size = false;
        node.rect = [at[0], at[1], size[0], size[1]];
        indices[i] = index;
        Ok(index)
    }
}

pub(super) fn project(
    doc: &mut Document,
    binding: &mut Binding,
    run: &str,
    summary: &str,
) -> Result<()> {
    let graph = Network::parse(summary)?;
    let sources: Vec<_> = binding
        .input_ids
        .iter()
        .filter_map(|id| doc.nodes.iter().position(|n| &n.id == id))
        .collect();
    if sources.is_empty() && !binding.input_ids.is_empty() {
        return Ok(());
    }
    let roots = graph.children(None);
    let width = roots
        .iter()
        .map(|&i| graph.size(i)[0])
        .fold(280.0, f32::max);
    let mut y = binding
        .answer_maps
        .iter()
        .flat_map(|m| &m.node_ids)
        .filter_map(|id| doc.nodes.iter().find(|n| &n.id == id))
        .map(|n| n.rect[1] + n.rect[3] + 80.0)
        .fold(binding.anchor[1], f32::max);
    let mut indices = vec![0; graph.nodes.len()];
    for row in roots.chunks(2) {
        for (column, &i) in row.iter().enumerate() {
            graph.create(
                doc,
                i,
                [binding.anchor[0] + column as f32 * (width + 180.0), y],
                &mut indices,
            )?;
        }
        y += row.iter().map(|&i| graph.size(i)[1]).fold(0.0, f32::max) + 140.0;
    }
    for edge in &graph.edges {
        let from = graph.nodes.iter().position(|n| n.id == edge.from).unwrap();
        let to = graph.nodes.iter().position(|n| n.id == edge.to).unwrap();
        let a = doc.nodes[indices[from]].rect;
        let b = doc.nodes[indices[to]].rect;
        let dx = b[0] + b[2] * 0.5 - a[0] - a[2] * 0.5;
        let dy = b[1] + b[3] * 0.5 - a[1] - a[3] * 0.5;
        let reciprocal = graph
            .edges
            .iter()
            .any(|e| e.from == edge.to && e.to == edge.from);
        let rates = if reciprocal && dx.abs() < dy.abs() {
            // Put the two directions on opposite sides instead of overlapping labels.
            let side = if dy > 0.0 { 1.0 } else { 0.0 };
            vec![[side, 0.5], [side, 0.5]]
        } else if dx.abs() >= dy.abs() {
            let side = if dx >= 0.0 { 1.0 } else { 0.0 };
            vec![[side, 0.5], [1.0 - side, 0.5]]
        } else {
            let side = if dy >= 0.0 { 1.0 } else { 0.0 };
            vec![[0.5, side], [0.5, 1.0 - side]]
        };
        doc.add_edge(crate::GraphEdge {
            id: id(),
            members: vec![indices[from], indices[to]],
            rates,
            text: edge.label.clone(),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
    }
    for &source in &sources {
        for &root in &roots {
            doc.add_edge(crate::GraphEdge {
                id: id(),
                members: vec![source, indices[root]],
                rates: vec![[1.0, 0.5], [0.0, 0.5]],
                ..Default::default()
            })
            .map_err(|e| e.to_string())?;
        }
    }
    binding.answer_maps.push(AnswerMap {
        run_id: run.into(),
        node_ids: indices.iter().map(|&i| doc.nodes[i].id.clone()).collect(),
    });
    doc.mark_project_changed();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const EXAMPLE: &str = r#"{"nodes":[
        {"id":"concepts","text":"核心概念"},
        {"id":"history","text":"历史与引用","parent":"concepts"},
        {"id":"commit","text":"提交：记录项目快照","parent":"history"},
        {"id":"branch","text":"分支：指向提交","parent":"history"},
        {"id":"actions","text":"日常操作"},
        {"id":"command","text":"git commit：创建提交","parent":"actions"}],
        "edges":[{"from":"command","to":"commit","label":"创建"},
        {"from":"branch","to":"commit","label":"指向"},
        {"from":"commit","to":"branch","label":"可被多个分支引用"}]}"#;

    #[test]
    fn nested_groups_and_cross_links_survive_save() {
        let mut doc = Document::blank().unwrap();
        let root = doc.create_text_node([0.0, 0.0]).unwrap();
        doc.graph.nodes[root].text = "Git".into();
        let mut binding = Binding {
            workflow_id: "test".into(),
            input_ids: vec![doc.nodes[root].id.clone()],
            anchor: [300.0, 0.0],
            agents: vec![],
            answer_maps: vec![],
        };
        project(&mut doc, &mut binding, "run", EXAMPLE).unwrap();
        assert_eq!(doc.nodes.len(), 7);
        assert_eq!(doc.edges.len(), 5);
        let by_text = |text: &str| doc.nodes.iter().position(|n| n.text == text).unwrap();
        let concepts = by_text("核心概念");
        let history = by_text("历史与引用");
        assert!(
            doc.nodes[concepts]
                .children
                .contains(&doc.nodes[history].id)
        );
        assert_eq!(doc.nodes[history].children.len(), 2);
        let commit = by_text("提交：记录项目快照");
        assert_eq!(
            doc.edges.iter().filter(|e| e.members[1] == commit).count(),
            2
        );
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("network.mio");
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        for node in &doc.nodes {
            let restored = reopened.nodes.iter().find(|n| n.id == node.id).unwrap();
            assert_eq!(restored.text, node.text);
            assert_eq!(restored.kind, node.kind);
            assert_eq!(restored.children, node.children);
        }
        for edge in &doc.edges {
            let restored = reopened.edges.iter().find(|e| e.id == edge.id).unwrap();
            assert_eq!(restored.text, edge.text);
            assert_eq!(
                restored
                    .members
                    .iter()
                    .map(|&i| &reopened.nodes[i].id)
                    .collect::<Vec<_>>(),
                edge.members
                    .iter()
                    .map(|&i| &doc.nodes[i].id)
                    .collect::<Vec<_>>()
            );
        }
        if let Some(path) = std::env::var_os("RAVOMI_NETWORK_FIXTURE") {
            doc.save(std::path::Path::new(&path)).unwrap();
        }
    }

    #[test]
    fn rejects_invalid_containment_and_dangling_edges() {
        assert!(
            Network::parse(r#"{"nodes":[{"id":"a","text":"A","parent":"a"}],"edges":[]}"#).is_err()
        );
        assert!(Network::parse(r#"{"nodes":[{"id":"a","text":"A"}],"edges":[{"from":"a","to":"missing","label":"关联"}]}"#).is_err());
        assert!(
            Network::parse(r#"{"nodes":[{"id":"a","text":"A"},{"id":"a","text":"B"}],"edges":[]}"#)
                .is_err()
        );
    }
}

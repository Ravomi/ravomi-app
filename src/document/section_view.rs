use super::*;

/// Indices remain document indices; hidden members resolve to their outermost collapsed group.
pub struct SectionView {
    pub representatives: Vec<usize>,
    pub rects: Vec<[f32; 4]>,
    pub edges: Vec<Option<GraphEdge>>,
}

impl SectionView {
    pub fn visible(&self, index: usize) -> bool {
        self.representatives.get(index) == Some(&index)
    }
}

impl Document {
    pub fn reveal_node(&mut self, index: usize) -> Result<bool> {
        if index >= self.nodes.len() {
            return Err("要定位的节点不存在".into());
        }
        // Validate the parent tree before walking it or changing any collapse flags.
        let representatives = self.section_representatives()?;
        if representatives[index] == index {
            return Ok(false);
        }
        let mut ancestors = HashSet::new();
        let mut child = index;
        while let Some(parent) = self.nodes.iter().position(|node| {
            node.kind == "Section" && node.children.contains(&self.nodes[child].id)
        }) {
            if self.nodes[parent].collapsed {
                ancestors.insert(parent);
            }
            child = parent;
        }
        self.set_sections_collapsed(&ancestors, false)
    }

    pub fn section_representatives(&self) -> Result<Vec<usize>> {
        let indices: BTreeMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.as_str(), index))
            .collect();
        if indices.len() != self.nodes.len() {
            return Err("节点 ID 重复，无法计算分组显示".into());
        }
        let mut parents = vec![None; self.nodes.len()];
        for (index, node) in self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == "Section")
        {
            for child in &node.children {
                let &child = indices.get(child.as_str()).ok_or("分组成员不存在")?;
                if parents[child].is_some_and(|parent| parent != index) {
                    return Err("节点同时属于多个分组，无法计算折叠显示".into());
                }
                parents[child] = Some(index);
            }
        }
        let mut representatives = Vec::with_capacity(self.nodes.len());
        for index in 0..self.nodes.len() {
            let mut representative = index;
            let mut parent = parents[index];
            let mut depth = 0;
            while let Some(ancestor) = parent {
                depth += 1;
                if depth >= self.nodes.len() {
                    return Err("分组成员存在循环引用".into());
                }
                if self.nodes[ancestor].collapsed {
                    representative = ancestor;
                }
                parent = parents[ancestor];
            }
            representatives.push(representative);
        }
        Ok(representatives)
    }

    pub fn section_view(
        &self,
        mut collapsed_size: impl FnMut(usize, &GraphNode) -> Result<[f32; 2]>,
    ) -> Result<SectionView> {
        let representatives = self.section_representatives()?;
        let mut rects: Vec<_> = self.nodes.iter().map(|node| node.rect).collect();
        for (index, node) in self.nodes.iter().enumerate() {
            if representatives[index] != index || node.kind != "Section" || !node.collapsed {
                continue;
            }
            let size = collapsed_size(index, node)?;
            if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
                return Err("折叠分组的显示尺寸无效".into());
            }
            let rect = [
                node.rect[0] + (node.rect[2] - size[0]) * 0.5,
                node.rect[1] + (node.rect[3] - size[1]) * 0.5,
                size[0],
                size[1],
            ];
            if rect.iter().any(|value| !value.is_finite()) {
                return Err("折叠分组的显示边界无效".into());
            }
            rects[index] = rect;
        }
        let edges = self
            .edges
            .iter()
            .map(|edge| -> Result<Option<GraphEdge>> {
                if edge.members.len() != edge.rates.len() {
                    return Err("连线端点与连接位置数量不一致".into());
                }
                let members = edge
                    .members
                    .iter()
                    .map(|&index| {
                        representatives
                            .get(index)
                            .copied()
                            .ok_or_else(|| "连线端点不存在".into())
                    })
                    .collect::<Result<Vec<_>>>()?;
                let projected = members != edge.members;
                if projected
                    && members
                        .first()
                        .is_some_and(|first| members.iter().all(|member| member == first))
                {
                    return Ok(None);
                }
                let mut display = edge.clone();
                display.members.clear();
                display.rates.clear();
                for (position, member) in members.into_iter().enumerate() {
                    if projected
                        && self.nodes[member].collapsed
                        && display.members.contains(&member)
                    {
                        continue;
                    }
                    display.members.push(member);
                    display.rates.push(if member == edge.members[position] {
                        edge.rates[position]
                    } else {
                        [0.5; 2]
                    });
                }
                Ok(Some(display))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(SectionView {
            representatives,
            rects,
            edges,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested() -> (Document, usize, usize) {
        let mut doc = Document::blank().unwrap();
        for x in [0.0, 300.0, 600.0, 900.0] {
            doc.create_text_node([x, 0.0]).unwrap();
        }
        for (id, members) in [
            ("internal", vec![0, 1]),
            ("external", vec![1, 3]),
            ("self", vec![3, 3]),
        ] {
            doc.add_edge(GraphEdge {
                id: id.into(),
                members,
                rates: vec![[0.2, 0.8]; 2],
                ..Default::default()
            })
            .unwrap();
        }
        let inner = doc.create_group(&HashSet::from([0, 1])).unwrap();
        let outer = doc.create_group(&HashSet::from([inner, 2])).unwrap();
        (doc, inner, outer)
    }

    #[test]
    fn revealing_a_hidden_node_expands_only_its_ancestors_in_one_history_entry() {
        let (mut doc, inner, outer) = nested();
        let unrelated = doc.create_group(&HashSet::from([3])).unwrap();
        doc.set_sections_collapsed(&HashSet::from([inner, outer, unrelated]), true)
            .unwrap();
        let before = doc.nodes.clone();
        let edges = doc.edges.clone();
        assert!(doc.reveal_node(0).unwrap());
        assert!(!doc.nodes[inner].collapsed && !doc.nodes[outer].collapsed);
        assert!(doc.nodes[unrelated].collapsed);
        assert_eq!(doc.section_representatives().unwrap()[0], 0);
        for (node, old) in doc.nodes.iter().zip(&before) {
            assert_eq!(node.rect, old.rect);
            assert_eq!(node.children, old.children);
        }
        assert_eq!(doc.edges, edges);
        let revision = doc.revision;
        assert!(!doc.reveal_node(0).unwrap());
        assert_eq!(doc.revision, revision);
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.redo();
        assert_eq!(doc.section_representatives().unwrap()[0], 0);
    }

    #[test]
    fn revealing_rejects_locked_ancestors_invalid_indices_and_pending_edits_atomically() {
        let (mut doc, inner, outer) = nested();
        doc.set_sections_collapsed(&HashSet::from([inner, outer]), true)
            .unwrap();
        doc.nodes[outer].locked = true;
        let before = doc.nodes.clone();
        let revision = doc.revision;
        assert!(doc.reveal_node(0).is_err());
        assert!(doc.reveal_node(doc.nodes.len()).is_err());
        assert_eq!(doc.nodes, before);
        assert_eq!(doc.revision, revision);
        doc.nodes[outer].locked = false;
        doc.begin("正在编辑");
        assert!(doc.reveal_node(0).is_err());
        assert!(doc.nodes[inner].collapsed && doc.nodes[outer].collapsed);
        assert!(doc.in_transaction());
        doc.cancel();
    }

    #[test]
    fn nested_collapses_project_external_edges_without_changing_document() {
        let (mut doc, inner, outer) = nested();
        doc.set_sections_collapsed(&HashSet::from([inner, outer]), true)
            .unwrap();
        let nodes = doc.nodes.clone();
        let edges = doc.edges.clone();
        let revision = doc.revision;
        let view = doc
            .section_view(|index, _| {
                assert_eq!(index, outer);
                Ok([120.0, 60.0])
            })
            .unwrap();
        assert_eq!(view.representatives, [outer, outer, outer, 3, outer, outer]);
        assert!(!view.visible(inner));
        assert!(view.visible(outer) && view.visible(3));
        assert!(!view.visible(99));
        assert!(view.edges[0].is_none());
        let external = view.edges[1].as_ref().unwrap();
        assert_eq!(external.members, [outer, 3]);
        assert_eq!(external.rates, [[0.5, 0.5], [0.2, 0.8]]);
        assert_eq!(view.edges[2].as_ref().unwrap(), &edges[2]);
        for axis in 0..2 {
            assert_eq!(
                view.rects[outer][axis] + view.rects[outer][axis + 2] * 0.5,
                nodes[outer].rect[axis] + nodes[outer].rect[axis + 2] * 0.5
            );
        }
        assert_eq!(doc.nodes, nodes);
        assert_eq!(doc.edges, edges);
        assert_eq!(doc.revision, revision);
        doc.set_sections_collapsed(&HashSet::from([outer]), false)
            .unwrap();
        let view = doc
            .section_view(|index, _| {
                assert_eq!(index, inner);
                Ok([80.0, 40.0])
            })
            .unwrap();
        assert_eq!(view.representatives, [inner, inner, 2, 3, inner, outer]);
        doc.set_sections_collapsed(&HashSet::from([inner]), false)
            .unwrap();
        let expanded = doc
            .section_view(|_, _| panic!("expanded groups need no collapsed size"))
            .unwrap();
        assert_eq!(
            expanded.rects,
            doc.nodes.iter().map(|node| node.rect).collect::<Vec<_>>()
        );
        assert_eq!(
            expanded.edges,
            doc.edges.iter().cloned().map(Some).collect::<Vec<_>>()
        );
    }

    #[test]
    fn multi_target_edges_deduplicate_projected_members_and_keep_original_ports() {
        let (mut doc, inner, _) = nested();
        doc.edges.push(GraphEdge {
            id: "multi".into(),
            kind: "MultiTargetUndirectedEdge".into(),
            members: vec![0, 1, 2, 3],
            rates: vec![[0.1, 0.9]; 4],
            text: "关系".into(),
            ..Default::default()
        });
        doc.set_sections_collapsed(&HashSet::from([inner]), true)
            .unwrap();
        let view = doc.section_view(|_, _| Ok([100.0, 50.0])).unwrap();
        let edge = view.edges.last().unwrap().as_ref().unwrap();
        assert_eq!(edge.members, [inner, 2, 3]);
        assert_eq!(edge.rates, [[0.5; 2], [0.1, 0.9], [0.1, 0.9]]);
        assert_eq!(edge.text, "关系");
        assert_eq!(doc.edges.last().unwrap().members, [0, 1, 2, 3]);
    }

    #[test]
    fn malformed_hierarchies_and_invalid_display_sizes_are_rejected() {
        let (mut doc, inner, outer) = nested();
        doc.set_sections_collapsed(&HashSet::from([outer]), true)
            .unwrap();
        for size in [[f32::NAN, 1.0], [1.0, 0.0], [-1.0, 1.0]] {
            assert!(doc.section_view(|_, _| Ok(size)).is_err());
        }
        let children = doc.nodes[inner].children.clone();
        let outer_id = doc.nodes[outer].id.clone();
        doc.nodes[inner].children.push(outer_id);
        assert!(doc.section_view(|_, _| Ok([1.0; 2])).is_err());
        doc.nodes[inner].children = children;
        doc.nodes[outer].children.push("missing".into());
        assert!(doc.section_view(|_, _| Ok([1.0; 2])).is_err());
    }
}

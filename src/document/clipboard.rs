use super::*;

#[derive(Default)]
pub struct GraphClipboard {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    attachments: BTreeMap<String, Vec<u8>>,
    last_target: Option<[f32; 2]>,
    repeats: u32,
}

impl Document {
    /// Capture selected nodes, group descendants, selected edge endpoints and internal edges.
    pub fn capture_selection_snapshot(
        &self,
        nodes: &HashSet<usize>,
        edges: &HashSet<usize>,
    ) -> Result<Self> {
        if self.in_transaction() {
            return Err("当前编辑尚未结束".into());
        }
        let mut included = nodes.clone();
        for &index in edges {
            let edge = self.edges.get(index).ok_or("导出的连线不存在")?;
            included.extend(edge.members.iter().copied());
        }
        let mut contents = GraphClipboard::default();
        if !contents.copy(self, &included)? {
            return Err("请先选择需要导出的内容".into());
        }
        let mut snapshot = self.capture_snapshot()?;
        snapshot.graph = Graph {
            nodes: contents.nodes,
            edges: contents.edges,
            attachments: contents.attachments,
            counts: BTreeMap::new(),
        };
        for kind in snapshot
            .graph
            .nodes
            .iter()
            .map(|node| &node.kind)
            .chain(snapshot.graph.edges.iter().map(|edge| &edge.kind))
        {
            *snapshot.graph.counts.entry(kind.clone()).or_default() += 1;
        }
        Ok(snapshot)
    }
}

impl GraphClipboard {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn copy(&mut self, document: &Document, selected: &HashSet<usize>) -> Result<bool> {
        let indices: BTreeMap<_, _> = document
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut included = HashSet::new();
        let mut pending: Vec<_> = selected.iter().copied().collect();
        while let Some(index) = pending.pop() {
            let node = document.nodes.get(index).ok_or("复制的节点不存在")?;
            if included.insert(index) {
                for id in &node.children {
                    pending.push(*indices.get(id.as_str()).ok_or("复制的分组成员不存在")?);
                }
            }
        }
        if included.is_empty() {
            return Ok(false);
        }
        let mut snapshot = Self::default();
        let mut remap = BTreeMap::new();
        for (index, node) in document.nodes.iter().enumerate() {
            if !included.contains(&index) {
                continue;
            }
            if let Some(name) = &node.attachment {
                snapshot.attachments.insert(
                    name.clone(),
                    document
                        .attachments
                        .get(name)
                        .ok_or("复制节点的附件不存在")?
                        .clone(),
                );
            }
            remap.insert(index, snapshot.nodes.len());
            snapshot.nodes.push(node.clone());
        }
        refreshed_group_bounds(&snapshot.nodes)?;
        snapshot.edges = document
            .edges
            .iter()
            .filter_map(|edge| {
                let members = edge
                    .members
                    .iter()
                    .map(|i| remap.get(i).copied())
                    .collect::<Option<Vec<_>>>()?;
                let mut edge = edge.clone();
                edge.members = members;
                Some(edge)
            })
            .collect();
        *self = snapshot;
        Ok(true)
    }

    pub fn cut(&mut self, document: &mut Document, selected: &HashSet<usize>) -> Result<bool> {
        let mut snapshot = Self::default();
        if !snapshot.copy(document, selected)? {
            return Ok(false);
        }
        let ids: HashSet<_> = snapshot.nodes.iter().map(|n| n.id.as_str()).collect();
        let indices = document
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| ids.contains(n.id.as_str()))
            .map(|(i, _)| i)
            .collect();
        let plan = document.deletion_plan(&indices, &HashSet::new())?;
        if plan.nodes != indices {
            return Err("选区包含锁定节点，无法剪切；可以复制".into());
        }
        // A failed validation keeps both the old clipboard and source document intact.
        document.delete_objects(&indices, &HashSet::new())?;
        *self = snapshot;
        Ok(true)
    }

    /// Paste centered at a document-space point. Return the new node indices.
    pub fn paste(&mut self, document: &mut Document, target: [f32; 2]) -> Result<HashSet<usize>> {
        if !target.iter().all(|v| v.is_finite()) {
            return Err("粘贴位置无效".into());
        }
        if self.is_empty() {
            return Ok(HashSet::new());
        }
        let repeats = if self.last_target == Some(target) {
            self.repeats.saturating_add(1)
        } else {
            0
        };
        let mut lo = [f32::INFINITY; 2];
        let mut hi = [f32::NEG_INFINITY; 2];
        for node in &self.nodes {
            for axis in 0..2 {
                lo[axis] = lo[axis].min(node.rect[axis]);
                hi[axis] = hi[axis].max(node.rect[axis] + node.rect[axis + 2]);
            }
        }
        let delta = [
            target[0] - (lo[0] + hi[0]) * 0.5 + repeats as f32 * 24.0,
            target[1] - (lo[1] + hi[1]) * 0.5 + repeats as f32 * 24.0,
        ];
        let mut reserved = document.reserved_ids.clone();
        reserved.extend(
            document
                .nodes
                .iter()
                .map(|n| n.id.clone())
                .chain(document.edges.iter().map(|e| e.id.clone())),
        );
        let mut new_id = || loop {
            let id = uuid::Uuid::new_v4().to_string();
            if reserved.insert(id.clone()) {
                break id;
            }
        };
        let ids: BTreeMap<_, _> = self
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), new_id()))
            .collect();
        let mut attachments = BTreeMap::new();
        for (name, bytes) in &self.attachments {
            let suffix = name.rsplit_once('.').ok_or("附件名称缺少扩展名")?.1;
            attachments.insert(
                name.as_str(),
                (format!("{}.{}", new_id(), suffix), bytes.clone()),
            );
        }
        let before = Structure::capture(&document.graph);
        let mut after = before.clone();
        let start = after.nodes.len();
        for original in &self.nodes {
            let mut node = original.clone();
            node.id = ids[original.id.as_str()].clone();
            for (axis, offset) in delta.iter().enumerate() {
                node.rect[axis] += offset;
            }
            if !node.rect.iter().all(|v| v.is_finite()) {
                return Err("粘贴后的节点位置无效".into());
            }
            node.children = node
                .children
                .iter()
                .map(|id| ids[id.as_str()].clone())
                .collect();
            node.attachment = node
                .attachment
                .as_ref()
                .map(|name| attachments[name.as_str()].0.clone());
            *after.counts.entry(node.kind.clone()).or_default() += 1;
            after.nodes.push(node);
        }
        for original in &self.edges {
            let mut edge = original.clone();
            edge.id = new_id();
            edge.members = edge.members.iter().map(|i| start + i).collect();
            *after.counts.entry(edge.kind.clone()).or_default() += 1;
            after.edges.push(edge);
        }
        let selected = (start..after.nodes.len()).collect();
        document.finish();
        for (_, (name, bytes)) in attachments {
            document.attachments.insert(name, bytes);
        }
        document.reserved_ids = reserved;
        document.history.edit(
            &mut document.graph,
            Change {
                label: "粘贴".into(),
                selection: Some((
                    document.selection.clone(),
                    Selection {
                        nodes: after.nodes[start..].iter().map(|n| n.id.clone()).collect(),
                        edges: HashSet::new(),
                    },
                )),
                before: vec![],
                after: vec![],
                added_node: None,
                added_edge: None,
                structure: Some((before, after)),
                initial_layouts: document.pending_layouts.values().cloned().collect(),
            },
        );
        document.revision += 1;
        self.last_target = Some(target);
        self.repeats = repeats;
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_collapsed_group_preserves_state_and_deletes_members_atomically() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0, 0.0]).unwrap();
        doc.create_text_node([300.0, 0.0]).unwrap();
        let group = doc.create_group(&HashSet::from([0, 1])).unwrap();
        doc.set_sections_collapsed(&HashSet::from([group]), true)
            .unwrap();
        let mut clipboard = GraphClipboard::default();
        clipboard.copy(&doc, &HashSet::from([group])).unwrap();
        let mut target = Document::blank().unwrap();
        clipboard.paste(&mut target, [1000.0, 1000.0]).unwrap();
        let group = target
            .nodes
            .iter()
            .position(|node| node.kind == "Section")
            .unwrap();
        assert!(target.nodes[group].collapsed);
        assert!(!target.section_overview_allowed(&target.nodes[group].id));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("copied.mio");
        target.save(&path).unwrap();
        assert!(same_nodes(
            &target.nodes,
            &Document::open(&path).unwrap().nodes
        ));
        target
            .delete_objects(&HashSet::from([group]), &HashSet::new())
            .unwrap();
        assert!(target.nodes.is_empty());
        target.undo();
        assert_eq!(target.nodes.len(), 3);
        assert!(target.nodes[group].collapsed);
    }

    #[test]
    fn selection_snapshot_keeps_nested_groups_and_remaps_edges_without_editing_source() {
        let mut doc = Document::blank().unwrap();
        for x in [0.0, 300.0, 600.0, 900.0] {
            doc.create_text_node([x, 0.0]).unwrap();
        }
        doc.nodes[2].kind = "ImageNode".into();
        doc.nodes[2].auto_size = false;
        doc.nodes[2].attachment = Some("selected.png".into());
        doc.attachments.insert("selected.png".into(), vec![1, 2, 3]);
        doc.attachments.insert("unused.png".into(), vec![4]);
        for members in [vec![1, 2], vec![0, 1], vec![2, 3]] {
            doc.add_edge(GraphEdge {
                id: uuid::Uuid::new_v4().to_string(),
                members,
                rates: vec![[0.5; 2]; 2],
                ..Default::default()
            })
            .unwrap();
        }
        let inner = doc.create_group(&HashSet::from([1, 2])).unwrap();
        let outer = doc.create_group(&HashSet::from([inner])).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mio");
        doc.save(&source).unwrap();
        let original_bytes = fs::read(&source).unwrap();
        doc.selection = Selection::capture(&doc, &HashSet::from([outer]), &HashSet::new());
        let selection = doc.selection.clone();
        let revision = doc.revision;
        let nodes = doc.nodes.clone();
        let edges = doc.edges.clone();
        let snapshot = doc
            .capture_selection_snapshot(&HashSet::from([outer]), &HashSet::new())
            .unwrap();
        assert_eq!(snapshot.nodes.len(), 4);
        assert_eq!(snapshot.edges.len(), 1);
        assert_eq!(snapshot.edges[0].members, [0, 1]);
        assert_eq!(snapshot.nodes[0], nodes[1]);
        assert_eq!(snapshot.nodes[1], nodes[2]);
        assert_eq!(snapshot.attachments.len(), 1);
        assert_eq!(snapshot.attachments["selected.png"], [1, 2, 3]);
        let output = dir.path().join("selection.mio");
        snapshot.save_snapshot(&output).unwrap();
        let reopened = Document::open(&output).unwrap();
        assert!(same_nodes(&snapshot.nodes, &reopened.nodes));
        assert!(same_edges(&snapshot, &reopened));
        assert_eq!(doc.nodes, nodes);
        assert_eq!(doc.edges, edges);
        assert_eq!(doc.selection, selection);
        assert_eq!(doc.revision, revision);
        assert!(!doc.dirty());
        assert_eq!(fs::read(&source).unwrap(), original_bytes);
        doc.undo();
        assert_eq!(doc.nodes.len(), nodes.len() - 1);
        doc.redo();
        assert_eq!(doc.nodes, nodes);
        assert!(!doc.dirty());
    }

    #[test]
    fn edge_selection_snapshot_includes_endpoints_and_rejects_invalid_selection() {
        let mut doc = Document::blank().unwrap();
        for x in [0.0, 300.0, 600.0] {
            doc.create_text_node([x, 0.0]).unwrap();
        }
        doc.add_edge(GraphEdge {
            id: "selected-edge".into(),
            members: vec![1, 2],
            rates: vec![[0.5; 2]; 2],
            ..Default::default()
        })
        .unwrap();
        let snapshot = doc
            .capture_selection_snapshot(&HashSet::new(), &HashSet::from([0]))
            .unwrap();
        assert_eq!(snapshot.nodes, doc.nodes[1..]);
        assert_eq!(snapshot.edges[0].members, [0, 1]);
        for (nodes, edges) in [
            (HashSet::new(), HashSet::new()),
            (HashSet::from([99]), HashSet::new()),
            (HashSet::new(), HashSet::from([99])),
        ] {
            assert!(doc.capture_selection_snapshot(&nodes, &edges).is_err());
        }
    }

    #[test]
    fn paste_and_delete_restore_stable_selections_through_history() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.selection = Selection::capture(&doc, &HashSet::from([0]), &HashSet::new());
        let original = doc.selection.clone();
        let mut clipboard = GraphClipboard::default();
        clipboard.copy(&doc, &HashSet::from([0])).unwrap();
        let pasted = clipboard.paste(&mut doc, [300.0, 0.0]).unwrap();
        let after = Selection::capture(&doc, &pasted, &HashSet::new());
        assert_ne!(original, after);
        doc.undo();
        assert_eq!(doc.take_restored_selection(), Some(original));
        doc.redo();
        assert_eq!(doc.take_restored_selection(), Some(after.clone()));
        doc.delete_objects(&pasted, &HashSet::new()).unwrap();
        doc.undo();
        assert_eq!(doc.take_restored_selection(), Some(after));
        doc.redo();
        assert_eq!(doc.take_restored_selection(), Some(Selection::default()));
    }

    #[test]
    fn attachments_and_multi_edge_styles_survive_paste_save_and_reopen() {
        let mut source = Document::blank().unwrap();
        source.create_text_node([0.0; 2]).unwrap();
        source.create_text_node([300.0, 0.0]).unwrap();
        source.nodes[0].kind = "ImageNode".into();
        source.nodes[0].auto_size = false;
        source.nodes[0].attachment = Some("source.png".into());
        source
            .attachments
            .insert("source.png".into(), vec![1, 2, 3]);
        source.edges.push(GraphEdge {
            id: "multi".into(),
            kind: "MultiTargetUndirectedEdge".into(),
            arrow: false,
            arrow_type: "none".into(),
            members: vec![0, 1],
            rates: vec![[0.1, 0.2], [0.8, 0.9]],
            render_type: "curve".into(),
            multi_arrow: "both".into(),
            center_rate: [0.3, 0.7],
            padding: 17.0,
            ..Default::default()
        });
        let mut clipboard = GraphClipboard::default();
        clipboard.copy(&source, &HashSet::from([0, 1])).unwrap();
        let mut target = Document::blank().unwrap();
        clipboard.paste(&mut target, [0.0; 2]).unwrap();
        let name = target.nodes[0].attachment.as_ref().unwrap();
        assert_ne!(name, "source.png");
        assert_eq!(target.attachments[name], [1, 2, 3]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("attachments.mio");
        target.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert!(same_nodes(&target.nodes, &reopened.nodes));
        assert!(same_edges(&target, &reopened));
        assert_eq!(target.attachments, reopened.attachments);
    }

    #[test]
    fn source_close_repeat_group_and_internal_edges_roundtrip() {
        let mut source = Document::blank().unwrap();
        source.create_text_node([0.0, 0.0]).unwrap();
        source.create_text_node([300.0, 0.0]).unwrap();
        source.create_text_node([700.0, 0.0]).unwrap();
        for (id, members) in [("internal", vec![0, 1]), ("external", vec![1, 2])] {
            source
                .add_edge(GraphEdge {
                    id: id.into(),
                    members,
                    rates: vec![[0.5; 2]; 2],
                    ..Default::default()
                })
                .unwrap();
        }
        let group = source.create_group(&HashSet::from([0, 1])).unwrap();
        let mut clipboard = GraphClipboard::default();
        clipboard.copy(&source, &HashSet::from([0, group])).unwrap();
        drop(source);
        let mut target = Document::blank().unwrap();
        let first = clipboard.paste(&mut target, [1000.0, 500.0]).unwrap();
        assert_eq!(first.len(), 3);
        assert_eq!(target.edges.len(), 1);
        assert_eq!(
            target.nodes[2].children,
            vec![target.nodes[0].id.clone(), target.nodes[1].id.clone()]
        );
        let second = clipboard.paste(&mut target, [1000.0, 500.0]).unwrap();
        assert!(first.is_disjoint(&second));
        assert_ne!(target.nodes[0].id, target.nodes[3].id);
        assert_eq!(target.nodes[3].rect[0], target.nodes[0].rect[0] + 24.0);
        assert_eq!(target.edges[1].members, [3, 4]);
        target.undo();
        assert_eq!(target.nodes.len(), 3);
        target.redo();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paste.mio");
        target.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert!(same_nodes(&target.nodes, &reopened.nodes));
        assert!(same_edges(&target, &reopened));
    }

    #[test]
    fn cut_and_paste_histories_are_independent_and_locked_cut_preserves_clipboard() {
        let mut source = Document::blank().unwrap();
        source.create_text_node([0.0; 2]).unwrap();
        let mut clipboard = GraphClipboard::default();
        clipboard.cut(&mut source, &HashSet::from([0])).unwrap();
        assert!(source.nodes.is_empty());
        let mut target = Document::blank().unwrap();
        clipboard.paste(&mut target, [0.0; 2]).unwrap();
        source.undo();
        assert_eq!(source.nodes.len(), 1);
        target.undo();
        assert_eq!(source.nodes.len(), 1);
        assert!(target.nodes.is_empty());
        source.nodes[0].locked = true;
        assert!(clipboard.cut(&mut source, &HashSet::from([0])).is_err());
        assert_eq!(clipboard.nodes.len(), 1);
        assert!(!clipboard.nodes[0].locked);
        assert_eq!(source.nodes.len(), 1);
    }
}

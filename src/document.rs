use crate::{Graph, GraphEdge, GraphNode, ProjectFile, Result};
use crate::{
    project::{self, Format, Manifest},
    project_history::History,
};
use rmpv::Value;
mod clipboard;
mod node_lock;
pub use clipboard::GraphClipboard;
mod section_view;
pub use section_view::SectionView;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Cursor, Read, Write},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

/// A creation snapshot shares its first measured size with history entries made
/// before fonts were ready. Corrections are relative, so an early drag is kept.
#[derive(Clone, Debug)]
struct InitialLayout {
    id: String,
    text: String,
    font_size: f32,
    size: [f32; 2],
    measured: Arc<OnceLock<[f32; 2]>>,
}
impl InitialLayout {
    fn matches(&self, node: &GraphNode) -> bool {
        node.id == self.id
            && node.kind == "TextNode"
            && node.auto_size
            && node.text == self.text
            && node.font_size == self.font_size
            && node.rect[2..4] == self.size
    }
    fn correct(&self, node: &mut GraphNode) {
        if self.matches(node)
            && let Some(&size) = self.measured.get()
        {
            node.rect[0] += (node.rect[2] - size[0]) * 0.5;
            node.rect[1] += (node.rect[3] - size[1]) * 0.5;
            node.rect[2..4].copy_from_slice(&size);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Change {
    pub label: String,
    before: Vec<GraphNode>,
    after: Vec<GraphNode>,
    added_edge: Option<GraphEdge>,
    added_node: Option<GraphNode>,
    structure: Option<(Structure, Structure)>,
    initial_layouts: Vec<InitialLayout>,
    selection: Option<(Selection, Selection)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub nodes: HashSet<String>,
    pub edges: HashSet<String>,
}
impl Selection {
    pub fn capture(graph: &Graph, nodes: &HashSet<usize>, edges: &HashSet<String>) -> Self {
        Self {
            nodes: nodes
                .iter()
                .filter_map(|&i| graph.nodes.get(i))
                .map(|n| n.id.clone())
                .collect(),
            edges: edges.clone(),
        }
    }
    fn retained(&self, nodes: &[GraphNode], edges: &[GraphEdge]) -> Self {
        Self {
            nodes: nodes
                .iter()
                .filter(|n| self.nodes.contains(&n.id))
                .map(|n| n.id.clone())
                .collect(),
            edges: edges
                .iter()
                .filter(|e| self.edges.contains(&e.id))
                .map(|e| e.id.clone())
                .collect(),
        }
    }
}
pub struct SelectionEffect(Option<Selection>);
#[derive(Clone, Debug)]
struct Structure {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    counts: BTreeMap<String, usize>,
}
impl Structure {
    fn capture(graph: &Graph) -> Self {
        Self {
            nodes: graph.nodes.clone(),
            edges: graph.edges.clone(),
            counts: graph.counts.clone(),
        }
    }
    fn restore(&self, graph: &mut Graph) {
        graph.nodes = self.nodes.clone();
        graph.edges = self.edges.clone();
        graph.counts = self.counts.clone();
    }
}
impl undo::Edit for Change {
    type Target = Graph;
    type Output = SelectionEffect;
    fn edit(&mut self, graph: &mut Graph) -> Self::Output {
        self.apply_initial_layouts();
        if let Some((_, after)) = &self.structure {
            after.restore(graph);
            return SelectionEffect(self.selection.as_ref().map(|(_, after)| after.clone()));
        }
        restore(graph, &self.after);
        if let Some(node) = &self.added_node {
            graph.nodes.push(node.clone());
            *graph.counts.entry(node.kind.clone()).or_default() += 1;
        }
        if let Some(edge) = &self.added_edge {
            graph.edges.push(edge.clone());
            *graph.counts.entry("LineEdge".into()).or_default() += 1;
        }
        SelectionEffect(self.selection.as_ref().map(|(_, after)| after.clone()))
    }
    fn undo(&mut self, graph: &mut Graph) -> Self::Output {
        self.apply_initial_layouts();
        if let Some((before, _)) = &self.structure {
            before.restore(graph);
            return SelectionEffect(self.selection.as_ref().map(|(before, _)| before.clone()));
        }
        restore(graph, &self.before);
        if let Some(node) = &self.added_node {
            graph.nodes.retain(|existing| existing.id != node.id);
            if let Some(count) = graph.counts.get_mut(&node.kind) {
                *count = count.saturating_sub(1);
            }
        }
        if let Some(edge) = &self.added_edge {
            graph.edges.retain(|existing| existing.id != edge.id);
            if let Some(count) = graph.counts.get_mut("LineEdge") {
                *count = count.saturating_sub(1);
            }
        }
        SelectionEffect(self.selection.as_ref().map(|(before, _)| before.clone()))
    }
}
impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
impl Change {
    fn apply_initial_layouts(&mut self) {
        for layout in &self.initial_layouts {
            for node in self
                .before
                .iter_mut()
                .chain(&mut self.after)
                .chain(&mut self.added_node)
            {
                layout.correct(node);
            }
            if let Some((before, after)) = &mut self.structure {
                for node in before.nodes.iter_mut().chain(&mut after.nodes) {
                    layout.correct(node);
                }
            }
        }
    }
}
fn restore(graph: &mut Graph, nodes: &[GraphNode]) {
    let by_id: BTreeMap<_, _> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for node in &mut graph.nodes {
        if let Some(saved) = by_id.get(node.id.as_str()) {
            if node.kind != saved.kind {
                if let Some(count) = graph.counts.get_mut(&node.kind) {
                    *count = count.saturating_sub(1);
                }
                *graph.counts.entry(saved.kind.clone()).or_default() += 1;
            }
            let layout_size = (node.auto_size == saved.auto_size
                && (node.auto_size || node.rect[2] == saved.rect[2])
                && node.kind == "TextNode"
                && saved.kind == "TextNode"
                && node.text == saved.text
                && node.font_size == saved.font_size)
                .then_some([node.rect[2], node.rect[3]]);
            *node = (*saved).clone();
            // A move undo must not restore an obsolete, asynchronously measured size.
            if let Some(size) = layout_size {
                node.rect[2..4].copy_from_slice(&size);
            }
        }
    }
}

pub struct Document {
    pub graph: Graph,
    pub history: undo::Record<Change>,
    pub path: Option<PathBuf>,
    pub revision: u64,
    pub selection: Selection,
    restored_selection: Option<Selection>,
    pending: Option<(String, Vec<GraphNode>)>,
    archive: Option<Vec<u8>>,
    manifest: Manifest,
    instance_token: uuid::Uuid,
    archived_history: History,
    project_dirty: bool,
    original: Vec<GraphNode>,
    original_edges: Vec<GraphEdge>,
    reserved_ids: HashSet<String>,
    disk_bytes: Option<Vec<u8>>,
    recovered: bool,
    overview_excluded: HashSet<String>,
    sync_members: Vec<Vec<String>>,
    pending_layouts: BTreeMap<String, InitialLayout>,
}

#[derive(Clone, Debug, Default)]
pub struct DeletePlan {
    pub nodes: HashSet<usize>,
    pub edges: HashSet<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct NodeProperties {
    pub color: Option<[f32; 4]>,
    pub font_size: Option<f32>,
    pub auto_size: Option<bool>,
    pub width: Option<f32>,
}

/// Coordinates use the document convention: positive y points down.
pub struct JumpPlan {
    pub roots: Vec<usize>,
    pub moving: HashSet<usize>,
    pub source_bounds: [f32; 4],
    pub destination_bounds: [f32; 4],
    pub delta: [f32; 2],
    nodes: Vec<GraphNode>,
}
impl Deref for Document {
    type Target = Graph;
    fn deref(&self) -> &Graph {
        &self.graph
    }
}
impl DerefMut for Document {
    fn deref_mut(&mut self) -> &mut Graph {
        &mut self.graph
    }
}
impl Document {
    pub fn blank() -> Result<Self> {
        let mut document = Self::memory(Graph {
            nodes: vec![],
            edges: vec![],
            attachments: BTreeMap::new(),
            counts: BTreeMap::new(),
        });
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive.start_file(
            crate::storage::STAGE_ENTRY,
            zip::write::SimpleFileOptions::default(),
        )?;
        rmpv::encode::write_value(&mut archive, &Value::Array(vec![]))?;
        archive.start_file(
            crate::storage::METADATA_ENTRY,
            zip::write::SimpleFileOptions::default(),
        )?;
        rmpv::encode::write_value(
            &mut archive,
            &Value::Map(vec![(
                "version".into(),
                crate::storage::LEGACY_DOCUMENT_VERSION.into(),
            )]),
        )?;
        archive.start_file(
            crate::storage::TAGS_ENTRY,
            zip::write::SimpleFileOptions::default(),
        )?;
        rmpv::encode::write_value(&mut archive, &Value::Array(vec![]))?;
        archive.start_file(
            crate::storage::REFERENCE_ENTRY,
            zip::write::SimpleFileOptions::default(),
        )?;
        rmpv::encode::write_value(
            &mut archive,
            &Value::Map(vec![
                ("sections".into(), Value::Map(vec![])),
                ("files".into(), Value::Array(vec![])),
            ]),
        )?;
        document.archive = Some(archive.finish()?.into_inner());
        Ok(document)
    }

    pub fn memory(graph: Graph) -> Self {
        let original = graph.nodes.clone();
        let original_edges = graph.edges.clone();
        let reserved_ids = graph
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .chain(graph.edges.iter().map(|e| e.id.clone()))
            .collect();
        let mut history = undo::Record::new();
        history.set_saved();
        Self {
            graph,
            history,
            original,
            original_edges,
            reserved_ids,
            path: None,
            archive: None,
            manifest: Manifest::default(),
            instance_token: uuid::Uuid::new_v4(),
            archived_history: History::default(),
            project_dirty: false,
            disk_bytes: None,
            recovered: false,
            overview_excluded: HashSet::new(),
            sync_members: vec![],
            pending_layouts: BTreeMap::new(),
            pending: None,
            revision: 0,
            selection: Selection::default(),
            restored_selection: None,
        }
    }
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)?;
        let contents = project::inspect(&mut zip::ZipArchive::new(Cursor::new(&bytes))?)?;
        let file = ProjectFile::from_bytes(&bytes)?;
        let objects = file.objects();
        let reserved_ids = objects.keys().cloned().collect();
        let overview_excluded = objects
            .iter()
            .filter(|(_, object)| {
                object["_"] == "Section"
                    && matches!(object["borderStyle"].as_str(), Some("dashed" | "none"))
            })
            .map(|(id, _)| id.clone())
            .collect();
        let sync_members = objects
            .values()
            .filter(|object| object["_"] == "SyncAssociation")
            .filter_map(|object| object["associationList"].as_array())
            .map(|members| {
                members
                    .iter()
                    .map(|member| {
                        file.resolve(member)?
                            .get("uuid")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                            .ok_or_else(|| "同步关系成员缺少ID".into())
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        drop(objects);
        let mut doc = Self::memory(Graph::from_file(file)?);
        doc.reserved_ids = reserved_ids;
        doc.overview_excluded = overview_excluded;
        doc.sync_members = sync_members;
        doc.manifest = contents.manifest;
        doc.archived_history = contents.history;
        doc.path = Some(fs::canonicalize(path)?);
        doc.archive = Some(bytes.clone());
        doc.disk_bytes = Some(bytes);
        Ok(doc)
    }
    pub fn dirty(&self) -> bool {
        self.recovered || self.project_dirty || self.pending.is_some() || !self.history.is_saved()
    }

    pub fn project_id(&self) -> &str {
        &self.manifest.project_id
    }

    /// Local ownership only: opening a copy never reconnects an active runtime.
    pub fn instance_token(&self) -> uuid::Uuid {
        self.instance_token
    }

    pub fn mark_project_changed(&mut self) {
        self.project_dirty = true;
        self.revision += 1;
    }

    pub fn history_archive(&self) -> &History {
        &self.archived_history
    }

    /// Capture evidence, not runnable jobs. Refreshing a workflow replaces only
    /// that workflow's archived snapshot and keeps the other selected workflows.
    pub fn attach_history(&mut self, history: History) -> Result<()> {
        history.validate()?;
        let mut next = self.archived_history.clone();
        for workflow in history.workflows {
            if let Some(previous) = next
                .workflows
                .iter_mut()
                .find(|item| item.id == workflow.id)
            {
                for old in &previous.runs {
                    let fresh = workflow
                        .runs
                        .iter()
                        .find(|run| run.id == old.id)
                        .ok_or("本机流程缺少项目已保存的运行，未覆盖较完整的协作历史")?;
                    if old.prompt != fresh.prompt
                        || old.input_files != fresh.input_files
                        || old.sources != fresh.sources
                        || old.task_id != fresh.task_id
                        || old.created_at != fresh.created_at
                        || old.retry_of != fresh.retry_of
                    {
                        return Err("同一运行的固定输入发生冲突，未覆盖协作历史".into());
                    }
                    if matches!(
                        old.status,
                        crate::project_history::ArchivedStatus::Succeeded
                            | crate::project_history::ArchivedStatus::Failed
                            | crate::project_history::ArchivedStatus::Cancelled
                    ) {
                        let mut reviewed = old.clone();
                        reviewed.review.clone_from(&fresh.review);
                        if &reviewed != fresh {
                            return Err("同一运行的已完成成果发生冲突，未覆盖协作历史".into());
                        }
                    }
                }
                *previous = workflow;
            } else {
                next.workflows.push(workflow);
            }
        }
        next.validate()?;
        if next == self.archived_history {
            return Ok(());
        }
        self.archived_history = next;
        self.mark_project_changed();
        Ok(())
    }

    pub fn open_recovery(path: &Path) -> Result<Self> {
        let mut document = Self::open(path)?;
        document.path = None;
        document.disk_bytes = None;
        document.recovered = true;
        Ok(document)
    }

    pub fn section_overview_allowed(&self, id: &str) -> bool {
        !self.overview_excluded.contains(id)
            && self
                .nodes
                .iter()
                .any(|node| node.id == id && !node.collapsed)
    }

    pub fn set_sections_collapsed(
        &mut self,
        selection: &HashSet<usize>,
        collapsed: bool,
    ) -> Result<bool> {
        if self.in_transaction() {
            return Err("请先结束当前编辑，再折叠或展开分组".into());
        }
        let protected = self.protected_nodes();
        for &index in selection {
            let node = self.nodes.get(index).ok_or("分组不存在")?;
            if node.kind != "Section" {
                return Err("只能折叠或展开分组".into());
            }
            if protected.contains(&index) {
                return Err("分组已锁定，无法折叠或展开".into());
            }
        }
        self.begin(if collapsed {
            "折叠分组"
        } else {
            "展开分组"
        });
        for &index in selection {
            self.nodes[index].collapsed = collapsed;
        }
        Ok(self.finish())
    }
    pub fn in_transaction(&self) -> bool {
        self.pending.is_some()
    }
    pub fn begin(&mut self, label: &str) {
        if self.pending.is_none() {
            self.pending = Some((label.to_owned(), self.nodes.clone()));
        }
    }
    pub fn finish(&mut self) -> bool {
        let Some((label, before)) = self.pending.take() else {
            return false;
        };
        let kinds: BTreeMap<_, _> = self
            .graph
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node.kind.as_str()))
            .collect();
        for old in &before {
            if let Some(&kind) = kinds.get(old.id.as_str())
                && old.kind != kind
            {
                if let Some(count) = self.graph.counts.get_mut(&old.kind) {
                    *count = count.saturating_sub(1);
                }
                *self.graph.counts.entry(kind.to_owned()).or_default() += 1;
            }
        }
        let current: BTreeMap<_, _> = self.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut change = Change {
            label,
            selection: None,
            before: vec![],
            after: vec![],
            added_edge: None,
            added_node: None,
            structure: None,
            initial_layouts: self.pending_layouts.values().cloned().collect(),
        };
        for node in before {
            if let Some(now) = current.get(node.id.as_str())
                && &node != *now
            {
                change.after.push((*now).clone());
                change.before.push(node);
            }
        }
        if change.before.is_empty() {
            return false;
        }
        self.history.edit(&mut self.graph, change);
        self.revision += 1;
        true
    }
    pub fn cancel(&mut self) {
        if let Some((_, before)) = self.pending.take() {
            restore(&mut self.graph, &before);
            self.revision += 1;
        }
    }
    pub fn undo(&mut self) {
        self.finish();
        if let Some(effect) = self.history.undo(&mut self.graph) {
            self.restored_selection = effect.0;
            if let Some(selection) = &self.restored_selection {
                self.selection = selection.clone();
            }
            self.restore_pending_layouts();
            self.revision += 1;
        }
    }
    pub fn redo(&mut self) {
        self.finish();
        if let Some(effect) = self.history.redo(&mut self.graph) {
            self.restored_selection = effect.0;
            if let Some(selection) = &self.restored_selection {
                self.selection = selection.clone();
            }
            self.restore_pending_layouts();
            self.revision += 1;
        }
    }
    pub fn take_restored_selection(&mut self) -> Option<Selection> {
        self.restored_selection.take()
    }
    fn restore_pending_layouts(&mut self) {
        self.pending_layouts.clear();
        let nodes: BTreeMap<_, _> = self.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut pending = BTreeMap::new();
        for entry in self.history.entries() {
            for layout in &entry.as_ref().initial_layouts {
                if layout.measured.get().is_none()
                    && nodes
                        .get(layout.id.as_str())
                        .is_some_and(|n| layout.matches(n))
                {
                    pending.insert(layout.id.clone(), layout.clone());
                }
            }
        }
        self.pending_layouts = pending;
    }
    pub fn add_edge(&mut self, mut edge: GraphEdge) -> Result<()> {
        if edge.id.is_empty()
            || self.reserved_ids.contains(&edge.id)
            || self.edges.iter().any(|existing| existing.id == edge.id)
        {
            return Err("连线 ID 为空或已被占用".into());
        }
        if edge.kind != "LineEdge" || edge.members.len() != 2 || edge.rates.len() != 2 {
            return Err("新建连线必须包含两个有效端点".into());
        }
        if edge.members.iter().any(|&index| index >= self.nodes.len())
            || edge
                .rates
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || edge
                .color
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("连线端点、连接位置或颜色无效".into());
        }
        if !edge.arrow {
            edge.arrow_type = "none".into();
        }
        edge.arrow = edge.arrow_type != "none";
        self.finish();
        self.history.edit(
            &mut self.graph,
            Change {
                label: "创建连线".into(),
                selection: None,
                before: vec![],
                after: vec![],
                added_edge: Some(edge),
                added_node: None,
                structure: None,
                initial_layouts: self.pending_layouts.values().cloned().collect(),
            },
        );
        self.revision += 1;
        Ok(())
    }
    pub fn protected_nodes(&self) -> HashSet<usize> {
        let indices: BTreeMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut protected = HashSet::new();
        let mut todo: Vec<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.locked)
            .map(|(i, _)| i)
            .collect();
        while let Some(index) = todo.pop() {
            if protected.insert(index) {
                todo.extend(
                    self.nodes[index]
                        .children
                        .iter()
                        .filter_map(|id| indices.get(id.as_str()))
                        .copied(),
                );
            }
        }
        protected
    }

    pub fn edit_edge(&mut self, index: usize, text: String, reverse: bool) -> Result<bool> {
        let edge = self.edges.get(index).ok_or("连线不存在")?;
        if reverse && (edge.kind != "LineEdge" || edge.members.len() != 2) {
            return Err("仅普通连线支持反转方向".into());
        }
        let protected = self.protected_nodes();
        if edge.members.iter().any(|i| protected.contains(i)) {
            return Err("锁定节点的连线不能修改".into());
        }
        let before = Structure::capture(&self.graph);
        let mut after = before.clone();
        let edge = &mut after.edges[index];
        edge.text = text;
        if reverse {
            edge.members.reverse();
            edge.rates.reverse();
        }
        if after.edges == before.edges {
            return Ok(false);
        }
        self.finish();
        self.history.edit(
            &mut self.graph,
            Change {
                label: if reverse {
                    "反转连线"
                } else {
                    "编辑连线标题"
                }
                .into(),
                selection: None,
                before: vec![],
                after: vec![],
                added_edge: None,
                added_node: None,
                structure: Some((before, after)),
                initial_layouts: self.pending_layouts.values().cloned().collect(),
            },
        );
        self.revision += 1;
        Ok(true)
    }

    pub fn set_node_properties(
        &mut self,
        selected: &HashSet<usize>,
        properties: &NodeProperties,
    ) -> Result<bool> {
        if selected.iter().any(|&i| i >= self.nodes.len()) {
            return Err("所选节点不存在".into());
        }
        if properties
            .color
            .is_some_and(|c| c.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
            || properties
                .font_size
                .is_some_and(|v| !v.is_finite() || !(2.0..=512.0).contains(&v))
            || properties
                .width
                .is_some_and(|v| !v.is_finite() || !(16.0..=100_000.0).contains(&v))
        {
            return Err("颜色、字号或宽度无效".into());
        }
        let protected = self.protected_nodes();
        if !selected.is_empty() && selected.is_subset(&protected) {
            return Err("锁定节点不能修改".into());
        }
        self.finish();
        self.begin("修改节点属性");
        for (index, node) in self.nodes.iter_mut().enumerate() {
            if !selected.contains(&index) || protected.contains(&index) {
                continue;
            }
            if let Some(color) = properties.color {
                node.color = color;
            }
            if node.kind == "TextNode" {
                if let Some(size) = properties.font_size {
                    node.font_size = size;
                }
                if let Some(width) = properties.width {
                    node.rect[2] = width;
                    node.auto_size = false;
                }
                if let Some(auto) = properties.auto_size {
                    node.auto_size = auto;
                }
            }
        }
        Ok(self.finish())
    }

    pub fn search_nodes(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return vec![];
        }
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.text.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn deletion_plan(
        &self,
        nodes: &HashSet<usize>,
        edges: &HashSet<usize>,
    ) -> Result<DeletePlan> {
        if nodes.iter().any(|&i| i >= self.nodes.len())
            || edges.iter().any(|&i| i >= self.edges.len())
        {
            return Err("待删除节点或连线不存在".into());
        }
        let protected = self.protected_nodes();
        let indices: BTreeMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut plan = DeletePlan::default();
        let mut todo: Vec<_> = nodes.iter().copied().collect();
        while let Some(index) = todo.pop() {
            if !protected.contains(&index)
                && plan.nodes.insert(index)
                && self.nodes[index].collapsed
            {
                todo.extend(
                    self.nodes[index]
                        .children
                        .iter()
                        .filter_map(|id| indices.get(id.as_str()))
                        .copied(),
                );
            }
        }
        for (index, edge) in self.edges.iter().enumerate() {
            if edge.members.iter().any(|&i| i >= self.nodes.len()) {
                return Err("连线包含无效端点".into());
            }
            let incident = edge.members.iter().any(|i| plan.nodes.contains(i));
            if incident
                || (edges.contains(&index) && edge.members.iter().all(|i| !protected.contains(i)))
            {
                plan.edges.insert(index);
            }
        }
        let affected: HashSet<_> = plan
            .edges
            .iter()
            .flat_map(|&i| self.edges[i].members.iter().copied())
            .collect();
        let connected: HashSet<_> = self
            .edges
            .iter()
            .enumerate()
            .filter(|(i, _)| !plan.edges.contains(i))
            .flat_map(|(_, edge)| edge.members.iter().copied())
            .collect();
        for index in affected {
            if self.nodes[index].kind == "ConnectPoint"
                && !protected.contains(&index)
                && !connected.contains(&index)
            {
                plan.nodes.insert(index);
            }
        }
        Ok(plan)
    }

    pub fn delete_objects(&mut self, nodes: &HashSet<usize>, edges: &HashSet<usize>) -> Result<()> {
        let plan = self.deletion_plan(nodes, edges)?;
        if plan.nodes.is_empty() && plan.edges.is_empty() {
            return Ok(());
        }
        let before = Structure::capture(&self.graph);
        let mut after = before.clone();
        let by_id: BTreeMap<_, _> = before
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        fn promote(
            id: &str,
            original: &[GraphNode],
            indices: &BTreeMap<&str, usize>,
            removed: &HashSet<usize>,
            visiting: &mut HashSet<usize>,
            out: &mut Vec<String>,
        ) -> Result<()> {
            let &index = indices.get(id).ok_or("分组成员不存在")?;
            if !removed.contains(&index) {
                if !out.iter().any(|existing| existing == id) {
                    out.push(id.to_owned());
                }
            } else if original[index].kind == "Section" {
                if !visiting.insert(index) {
                    return Err("分组成员存在循环引用".into());
                }
                for child in &original[index].children {
                    promote(child, original, indices, removed, visiting, out)?;
                }
                visiting.remove(&index);
            }
            Ok(())
        }
        let mut remap = vec![None; before.nodes.len()];
        let mut kept = vec![];
        for (index, node) in before.nodes.iter().enumerate() {
            if plan.nodes.contains(&index) {
                if let Some(count) = after.counts.get_mut(&node.kind) {
                    *count = count.saturating_sub(1);
                }
                continue;
            }
            remap[index] = Some(kept.len());
            let mut node = node.clone();
            let mut children = vec![];
            for child in &node.children {
                promote(
                    child,
                    &before.nodes,
                    &by_id,
                    &plan.nodes,
                    &mut HashSet::new(),
                    &mut children,
                )?;
            }
            node.children = children;
            let previous_kind = node.kind.clone();
            sync_container_kind(&mut node);
            if node.kind != previous_kind {
                if let Some(count) = after.counts.get_mut(&previous_kind) {
                    *count = count.saturating_sub(1);
                }
                *after.counts.entry(node.kind.clone()).or_default() += 1;
            }
            kept.push(node);
        }
        after.nodes = kept;
        after.edges.clear();
        for (index, edge) in before.edges.iter().enumerate() {
            if plan.edges.contains(&index) {
                if let Some(count) = after.counts.get_mut(&edge.kind) {
                    *count = count.saturating_sub(1);
                }
            } else {
                let mut edge = edge.clone();
                edge.members = edge
                    .members
                    .iter()
                    .map(|&i| remap[i].ok_or_else(|| "删除后连线端点丢失".into()))
                    .collect::<Result<Vec<_>>>()?;
                after.edges.push(edge);
            }
        }
        let live_ids: HashSet<_> = after.nodes.iter().map(|n| n.id.as_str()).collect();
        for members in &self.sync_members {
            let was_live = members
                .iter()
                .filter(|id| by_id.contains_key(id.as_str()))
                .count()
                >= 2;
            let remains_live = members
                .iter()
                .filter(|id| live_ids.contains(id.as_str()))
                .count()
                >= 2;
            if was_live
                && !remains_live
                && let Some(count) = after.counts.get_mut("SyncAssociation")
            {
                *count = count.saturating_sub(1);
            }
        }
        let bounds = refreshed_group_bounds(&after.nodes)?;
        for (node, bounds) in after.nodes.iter_mut().zip(bounds) {
            if node.kind == "Section" {
                node.rect = bounds;
            }
        }
        self.finish();
        self.history.edit(
            &mut self.graph,
            Change {
                label: "切割删除".into(),
                selection: Some((
                    self.selection.clone(),
                    self.selection.retained(&after.nodes, &after.edges),
                )),
                before: vec![],
                after: vec![],
                added_edge: None,
                added_node: None,
                structure: Some((before, after)),
                initial_layouts: self.pending_layouts.values().cloned().collect(),
            },
        );
        self.revision += 1;
        Ok(())
    }

    /// Create an automatically sized node centered in document coordinates.
    pub fn create_text_node(&mut self, center: [f32; 2]) -> Result<usize> {
        self.create_node(center, [60.0, 57.0], "TextNode", None, None)
    }

    /// Insert a decoded image; the caller validates format and supplies display size.
    pub fn import_image(
        &mut self,
        center: [f32; 2],
        size: [f32; 2],
        filename: &str,
        bytes: Vec<u8>,
    ) -> Result<usize> {
        if filename.contains(['/', '\\'])
            || filename.is_empty()
            || bytes.is_empty()
            || bytes.len() > 64 * 1024 * 1024
            || size.iter().any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err("图片名称、内容或尺寸无效".into());
        }
        let extension = filename
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let kind = match extension.as_str() {
            "svg" => "SvgNode",
            "png" | "jpg" | "jpeg" => "ImageNode",
            _ => return Err("支持导入 PNG、JPEG 和 SVG 图片".into()),
        };
        let name = loop {
            let name = format!("{}.{}", uuid::Uuid::new_v4(), extension);
            if !self.attachments.contains_key(&name) {
                break name;
            }
        };
        let index = self.create_node(
            center,
            size,
            kind,
            Some(name.clone()),
            Some(filename.to_owned()),
        )?;
        // Keep immutable attachment bytes across undo, as for cross-document paste.
        self.attachments.insert(name, bytes);
        Ok(index)
    }

    fn create_node(
        &mut self,
        center: [f32; 2],
        size: [f32; 2],
        kind: &str,
        attachment: Option<String>,
        source_name: Option<String>,
    ) -> Result<usize> {
        if !center.iter().all(|v| v.is_finite()) {
            return Err("新建节点位置无效".into());
        }
        // Validate cycles/missing members before inspecting ancestor locks.
        refreshed_group_bounds(&self.nodes)?;
        let indices: BTreeMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut parents = vec![vec![]; self.nodes.len()];
        for (index, node) in self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == "Section")
        {
            for child in &node.children {
                parents[indices[child.as_str()]].push(index);
            }
        }
        fn depth(index: usize, parents: &[Vec<usize>], cache: &mut [Option<usize>]) -> usize {
            if let Some(value) = cache[index] {
                return value;
            }
            let value = parents[index]
                .iter()
                .map(|&p| depth(p, parents, cache) + 1)
                .max()
                .unwrap_or(0);
            cache[index] = Some(value);
            value
        }
        let mut depths = vec![None; self.nodes.len()];
        let representatives = self.section_representatives()?;
        let parent = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| {
                let [x, y, w, h] = n.rect;
                n.kind == "Section"
                    && !n.collapsed
                    && representatives[*i] == *i
                    && center[0] >= x
                    && center[0] <= x + w
                    && center[1] >= y
                    && center[1] <= y + h
            })
            .map(|(i, n)| {
                (
                    i,
                    depth(i, &parents, &mut depths),
                    n.rect[2] as f64 * n.rect[3] as f64,
                )
            })
            .max_by(|a, b| {
                a.1.cmp(&b.1)
                    .then_with(|| b.2.total_cmp(&a.2))
                    .then_with(|| b.0.cmp(&a.0))
            })
            .map(|(i, _, _)| i);
        if parent.is_some_and(|i| self.protected_nodes().contains(&i)) {
            return Err("锁定分组或其子分组中不能新建节点".into());
        }
        let id = loop {
            let id = uuid::Uuid::new_v4().to_string();
            if !self.reserved_ids.contains(&id)
                && !self.nodes.iter().any(|n| n.id == id)
                && !self.edges.iter().any(|e| e.id == id)
            {
                break id;
            }
        };
        let before = Structure::capture(&self.graph);
        let mut after = before.clone();
        let index = after.nodes.len();
        if let Some(parent) = parent {
            after.nodes[parent].children.push(id.clone());
        }
        after.nodes.push(GraphNode {
            id,
            kind: kind.into(),
            rect: [
                center[0] - size[0] * 0.5,
                center[1] - size[1] * 0.5,
                size[0],
                size[1],
            ],
            text: if kind == "TextNode" {
                "...".into()
            } else {
                String::new()
            },
            font_size: 24.0,
            url: None,
            auto_size: kind == "TextNode",
            color: [0.0; 4],
            attachment,
            source_name,
            children: vec![],
            locked: false,
            collapsed: false,
            stroke: vec![],
        });
        *after.counts.entry(kind.into()).or_default() += 1;
        let bounds = refreshed_group_bounds(&after.nodes)?;
        for (node, bounds) in after.nodes.iter_mut().zip(bounds) {
            if node.kind == "Section" {
                node.rect = bounds;
            }
        }
        self.finish();
        let node = &after.nodes[index];
        if kind == "TextNode" {
            self.pending_layouts.insert(
                node.id.clone(),
                InitialLayout {
                    id: node.id.clone(),
                    text: node.text.clone(),
                    font_size: node.font_size,
                    size: [node.rect[2], node.rect[3]],
                    measured: Arc::new(OnceLock::new()),
                },
            );
        }
        self.history.edit(
            &mut self.graph,
            Change {
                label: if kind == "TextNode" {
                    "创建文字节点"
                } else {
                    "导入图片"
                }
                .into(),
                selection: Some((
                    self.selection.clone(),
                    Selection {
                        nodes: HashSet::from([after.nodes[index].id.clone()]),
                        edges: HashSet::new(),
                    },
                )),
                before: vec![],
                after: vec![],
                added_edge: None,
                added_node: None,
                structure: Some((before, after)),
                initial_layouts: self.pending_layouts.values().cloned().collect(),
            },
        );
        self.revision += 1;
        Ok(index)
    }

    pub fn create_group(&mut self, selection: &HashSet<usize>) -> Result<usize> {
        if selection.is_empty() || selection.iter().any(|&i| i >= self.nodes.len()) {
            return Err("请先选择需要分组的节点".into());
        }
        let indices: BTreeMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let descendants = |index: usize| {
            let mut result = HashSet::new();
            let mut todo = self.nodes[index].children.clone();
            while let Some(id) = todo.pop() {
                if let Some(&i) = indices.get(id.as_str())
                    && result.insert(i)
                {
                    todo.extend(self.nodes[i].children.iter().cloned());
                }
            }
            result
        };
        if self.nodes.iter().enumerate().any(|(i, n)| {
            n.locked && (selection.contains(&i) || !selection.is_disjoint(&descendants(i)))
        }) {
            return Err("锁定节点或锁定分组中的节点不能重新分组".into());
        }
        let mut roots = selection.clone();
        for &index in selection {
            for descendant in descendants(index) {
                roots.remove(&descendant);
            }
        }
        if roots.is_empty() {
            return Err("分组成员存在循环引用".into());
        }
        let mut roots: Vec<_> = roots.into_iter().collect();
        roots.sort_unstable();
        let root_ids: HashSet<_> = roots.iter().map(|&i| self.nodes[i].id.clone()).collect();
        let common_parent = self.nodes.iter().position(|n| {
            n.kind == "Section"
                && !root_ids.contains(&n.id)
                && root_ids.iter().all(|id| n.children.contains(id))
        });
        let id = loop {
            let id = uuid::Uuid::new_v4().to_string();
            if !self.reserved_ids.contains(&id)
                && !self.nodes.iter().any(|n| n.id == id)
                && !self.edges.iter().any(|e| e.id == id)
            {
                break id;
            }
        };
        let rect = group_bounds(roots.iter().map(|&i| self.nodes[i].rect), true)?;
        let group = GraphNode {
            id: id.clone(),
            kind: "Section".into(),
            rect,
            text: "分组".into(),
            url: None,
            font_size: 32.0,
            auto_size: false,
            color: [0.0; 4],
            attachment: None,
            source_name: None,
            children: roots.iter().map(|&i| self.nodes[i].id.clone()).collect(),
            locked: false,
            collapsed: false,
            stroke: vec![],
        };
        let mut updated = self.nodes.clone();
        let mut pending_parents = vec![];
        for (index, node) in updated.iter_mut().enumerate() {
            if node.kind != "Section" || !node.children.iter().any(|id| root_ids.contains(id)) {
                continue;
            }
            node.children.retain(|id| !root_ids.contains(id));
            if Some(index) == common_parent {
                node.children.push(id.clone());
            }
            pending_parents.push(index);
        }
        let index = updated.len();
        updated.push(group.clone());
        for node in &mut updated {
            sync_container_kind(node);
        }
        let by_id: BTreeMap<_, _> = updated
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let mut remaining = updated.len().saturating_mul(updated.len()).max(1);
        while let Some(index) = pending_parents.pop() {
            if remaining == 0 {
                return Err("分组存在循环引用，无法更新边界".into());
            }
            remaining -= 1;
            if updated[index].children.is_empty() {
                continue;
            }
            let rect = group_bounds(
                updated[index]
                    .children
                    .iter()
                    .filter_map(|id| by_id.get(id))
                    .map(|&i| updated[i].rect),
                !updated[index].text.is_empty(),
            )?;
            if updated[index].rect != rect {
                updated[index].rect = rect;
                let id = &updated[index].id;
                pending_parents.extend(
                    updated
                        .iter()
                        .enumerate()
                        .filter(|(_, n)| n.kind == "Section" && n.children.contains(id))
                        .map(|(i, _)| i),
                );
            }
        }
        let mut change = Change {
            label: "创建分组".into(),
            selection: Some((
                self.selection.clone(),
                Selection {
                    nodes: HashSet::from([group.id.clone()]),
                    edges: HashSet::new(),
                },
            )),
            before: vec![],
            after: vec![],
            added_edge: None,
            added_node: Some(group),
            structure: None,
            initial_layouts: self.pending_layouts.values().cloned().collect(),
        };
        for (old, new) in self.nodes.iter().zip(updated) {
            if old != &new {
                change.before.push(old.clone());
                change.after.push(new);
            }
        }
        self.finish();
        self.history.edit(&mut self.graph, change);
        self.revision += 1;
        Ok(index)
    }
    /// Plan a jump without changing the document, including its history or pending transaction.
    /// `None` moves the roots to the top level; a text destination becomes a container.
    pub fn jump_plan(
        &self,
        selected: &HashSet<usize>,
        destination: Option<usize>,
        center: [f32; 2],
    ) -> Result<JumpPlan> {
        if selected.is_empty() || selected.iter().any(|&i| i >= self.nodes.len()) {
            return Err("请先选择需要跳跃的节点".into());
        }
        if !center.iter().all(|v| v.is_finite()) {
            return Err("跳跃目标坐标必须有限".into());
        }
        if let Some(index) = destination {
            let node = self.nodes.get(index).ok_or("目标分组不存在")?;
            if !matches!(node.kind.as_str(), "TextNode" | "Section") {
                return Err("此节点内容暂不支持容纳子节点".into());
            }
        }
        // Validate the existing hierarchy before deriving roots or ancestor locks.
        refreshed_group_bounds(&self.nodes)?;
        let indices: BTreeMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut parents = vec![vec![]; self.nodes.len()];
        for (index, node) in self.nodes.iter().enumerate() {
            if node.kind != "Section" && !node.children.is_empty() {
                return Err("普通节点包含分组成员，暂不支持此容器格式".into());
            }
            for child in &node.children {
                let &child = indices.get(child.as_str()).ok_or("分组成员不存在")?;
                parents[child].push(index);
            }
        }
        let mut moving = HashSet::new();
        let mut todo: Vec<_> = selected.iter().copied().collect();
        while let Some(index) = todo.pop() {
            if moving.insert(index) {
                todo.extend(
                    self.nodes[index]
                        .children
                        .iter()
                        .map(|id| indices[id.as_str()]),
                );
            }
        }
        if destination.is_some_and(|index| moving.contains(&index)) {
            return Err("不能放入自身或内部节点".into());
        }
        let mut checked = HashSet::new();
        let mut todo: Vec<_> = moving.iter().copied().chain(destination).collect();
        while let Some(index) = todo.pop() {
            if checked.insert(index) {
                if self.nodes[index].locked {
                    return Err("锁定节点、锁定分组或其成员不能重新归组".into());
                }
                todo.extend(parents[index].iter().copied());
            }
        }
        let mut roots: Vec<_> = selected
            .iter()
            .copied()
            .filter(|&i| !parents[i].iter().any(|p| moving.contains(p)))
            .collect();
        roots.sort_unstable();
        if roots.is_empty() {
            return Err("分组成员存在循环引用".into());
        }
        let mut lo = [f32::INFINITY; 2];
        let mut hi = [f32::NEG_INFINITY; 2];
        for &index in &moving {
            let rect = self.nodes[index].rect;
            if !rect.iter().all(|v| v.is_finite()) || rect[2] <= 0.0 || rect[3] <= 0.0 {
                return Err("所选节点边界无效".into());
            }
            for axis in 0..2 {
                lo[axis] = lo[axis].min(rect[axis]);
                hi[axis] = hi[axis].max(rect[axis] + rect[axis + 2]);
            }
        }
        let source_bounds = [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]];
        let delta = [
            center[0] - (lo[0] * 0.5 + hi[0] * 0.5),
            center[1] - (lo[1] * 0.5 + hi[1] * 0.5),
        ];
        let destination_bounds = [
            lo[0] + delta[0],
            lo[1] + delta[1],
            source_bounds[2],
            source_bounds[3],
        ];
        if !source_bounds
            .iter()
            .chain(&delta)
            .chain(&destination_bounds)
            .all(|v| v.is_finite())
        {
            return Err("跳跃坐标超出可表示范围".into());
        }
        let mut nodes = self.nodes.clone();
        for &index in &moving {
            for (axis, offset) in delta.iter().enumerate() {
                nodes[index].rect[axis] += offset;
                if !nodes[index].rect[axis].is_finite()
                    || !(nodes[index].rect[axis] + nodes[index].rect[axis + 2]).is_finite()
                {
                    return Err("跳跃坐标超出可表示范围".into());
                }
            }
        }
        let root_ids: HashSet<_> = roots.iter().map(|&i| self.nodes[i].id.as_str()).collect();
        for node in &mut nodes {
            node.children.retain(|id| !root_ids.contains(id.as_str()));
        }
        if let Some(index) = destination {
            nodes[index]
                .children
                .extend(roots.iter().map(|&i| self.nodes[i].id.clone()));
        }
        for node in &mut nodes {
            sync_container_kind(node);
        }
        let bounds = refreshed_group_bounds(&nodes)?;
        for (index, (node, rect)) in nodes.iter_mut().zip(bounds).enumerate() {
            if node.kind == "Section" && checked.contains(&index) {
                node.rect = rect;
            }
        }
        Ok(JumpPlan {
            roots,
            moving,
            source_bounds,
            destination_bounds,
            delta,
            nodes,
        })
    }

    pub fn jump_selection(
        &mut self,
        selected: &HashSet<usize>,
        destination: Option<usize>,
        center: [f32; 2],
    ) -> Result<()> {
        let plan = self.jump_plan(selected, destination, center)?;
        // Preserve a pending drag as the start of this same jump transaction.
        self.begin("跳跃并重新归组");
        self.graph.nodes = plan.nodes;
        self.finish();
        Ok(())
    }

    pub fn refresh_group_bounds(&mut self) -> Result<bool> {
        let bounds = refreshed_group_bounds(&self.nodes)?;
        let mut changed = false;
        for (node, rect) in self.nodes.iter_mut().zip(bounds) {
            if node.kind == "Section"
                && node
                    .rect
                    .iter()
                    .zip(rect)
                    .any(|(a, b)| (*a - b).abs() > 0.01)
            {
                node.rect = rect;
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
        Ok(changed)
    }
    pub fn can_edit_text(&self, index: usize) -> bool {
        self.nodes.get(index).is_some_and(|node| {
            matches!(node.kind.as_str(), "TextNode" | "Section" | "LatexNode")
                && !self.protected_nodes().contains(&index)
        })
    }

    pub fn edit_text(&mut self, id: &str, text: String, size: [f32; 2]) -> bool {
        if !size.iter().all(|n| n.is_finite() && *n > 0.0) {
            return false;
        }
        let Some(index) = self
            .nodes
            .iter()
            .position(|n| n.id == id)
            .filter(|&index| self.can_edit_text(index))
        else {
            return false;
        };
        self.finish();
        self.begin("编辑文字");
        self.nodes[index].text = text;
        if self.nodes[index].kind == "TextNode" {
            self.nodes[index].rect[2..4].copy_from_slice(&size);
        }
        self.finish()
    }
    pub fn apply_layout_size(&mut self, index: usize, mut size: [f32; 2]) -> bool {
        if self.in_transaction() || !size.iter().all(|v| v.is_finite() && *v > 0.0) {
            return false;
        }
        let Some(node) = self.graph.nodes.get_mut(index) else {
            return false;
        };
        if node.kind != "TextNode" {
            return false;
        }
        if !node.auto_size {
            size[0] = node.rect[2];
        }
        let changed = node.rect[2..4]
            .iter()
            .zip(size)
            .any(|(old, new)| (*old - new).abs() > 0.01);
        if let Some(layout) = self.pending_layouts.remove(&node.id)
            && layout.matches(node)
        {
            let measured = if changed {
                size
            } else {
                [node.rect[2], node.rect[3]]
            };
            layout
                .measured
                .set(measured)
                .expect("initial layout is measured only once");
            layout.correct(node);
        }
        if !changed {
            return false;
        }
        node.rect[2..4].copy_from_slice(&size);
        self.revision += 1;
        true
    }
    pub fn expand_selection(&self, selected: &HashSet<usize>) -> HashSet<usize> {
        let protected = self.protected_nodes();
        let indices: BTreeMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut expanded = HashSet::new();
        let mut todo: Vec<_> = selected.iter().copied().collect();
        while let Some(i) = todo.pop() {
            let Some(node) = self.nodes.get(i) else {
                continue;
            };
            if protected.contains(&i) || !expanded.insert(i) {
                continue;
            }
            todo.extend(
                node.children
                    .iter()
                    .filter_map(|id| indices.get(id.as_str()).copied()),
            );
        }
        expanded
    }
    pub fn save(&mut self, destination: &Path) -> Result<()> {
        let normalized = project::save_path(destination)?;
        let destination = normalized.as_path();
        let absolute = if destination.exists() {
            fs::canonicalize(destination)?
        } else {
            let parent = destination
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::canonicalize(parent)?.join(destination.file_name().ok_or("保存路径缺少文件名")?)
        };
        let destination = absolute.as_path();
        if self.in_transaction() {
            return Err("请先结束当前编辑或拖动，再保存".into());
        }
        if self.path.as_deref() == Some(destination)
            && fs::read(destination).ok().as_ref() != self.disk_bytes.as_ref()
        {
            return Err("磁盘文件已被其他程序更改，请另存为以保留两个版本".into());
        }
        self.disk_bytes = Some(self.write_archive(destination, true)?);
        self.path = Some(destination.to_path_buf());
        self.history.set_saved();
        self.project_dirty = false;
        self.recovered = false;
        Ok(())
    }

    /// Write a recovery generation without changing the document's save point.
    /// Existing destinations are never replaced, including the source document.
    pub fn save_snapshot(&self, destination: &Path) -> Result<()> {
        if self.in_transaction() {
            return Err("请先结束当前编辑或拖动，再创建恢复副本".into());
        }
        self.write_archive(destination, false)?;
        Ok(())
    }

    pub fn capture_snapshot(&self) -> Result<Self> {
        if self.in_transaction() {
            return Err("当前编辑尚未结束".into());
        }
        let mut snapshot = Self::memory(Graph {
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
            attachments: self.attachments.clone(),
            counts: self.counts.clone(),
        });
        snapshot.archive = self.archive.clone();
        snapshot.manifest = self.manifest.clone();
        snapshot.archived_history = self.archived_history.clone();
        snapshot.project_dirty = self.project_dirty;
        snapshot.original = self.original.clone();
        snapshot.original_edges = self.original_edges.clone();
        snapshot.overview_excluded = self.overview_excluded.clone();
        snapshot.path = self.path.clone();
        snapshot.revision = self.revision;
        Ok(snapshot)
    }

    fn write_archive(&self, destination: &Path, replace: bool) -> Result<Vec<u8>> {
        let bytes = self
            .archive
            .as_ref()
            .ok_or("此文档没有可保存的原始 PRG 数据")?;
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let source = project::inspect(&mut archive)?.format;
        let target = Format::for_destination(destination)?;
        if target == Format::Prg
            && (!self.archived_history.workflows.is_empty() || !self.manifest.extensions.is_empty())
        {
            return Err("项目含协作历史或扩展信息，请保存为 .mio 以保留完整内容".into());
        }
        let mut stage = Vec::new();
        archive.by_name(source.stage())?.read_to_end(&mut stage)?;
        let mut value = rmpv::decode::read_value(&mut Cursor::new(stage))?;
        let before: BTreeMap<_, _> = self.original.iter().map(|n| (n.id.as_str(), n)).collect();
        let after: BTreeMap<_, _> = self.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        append_nodes(&mut value, &self.original, &self.nodes)?;
        patch_stage(&mut value, &before, &after)?;
        append_edges(
            &mut value,
            &self.original_edges,
            &self.edges,
            &self.original,
            &self.nodes,
        )?;
        let live: HashSet<_> = self
            .nodes
            .iter()
            .map(|n| n.id.as_str())
            .chain(self.edges.iter().map(|e| e.id.as_str()))
            .collect();
        let removed: HashSet<_> = self
            .original
            .iter()
            .map(|n| n.id.as_str())
            .chain(self.original_edges.iter().map(|e| e.id.as_str()))
            .filter(|id| !live.contains(id))
            .collect();
        if !removed.is_empty() {
            value = prune_deleted_stage(&value, &removed)?;
        }
        let mut stage = Vec::new();
        rmpv::encode::write_value(&mut stage, &value)?;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        {
            let mut writer = zip::ZipWriter::new(temp.as_file_mut());
            let mut names = HashSet::new();
            if target == Format::Mio {
                self.manifest.validate()?;
                self.archived_history.validate()?;
                writer.start_file(
                    project::MANIFEST_ENTRY,
                    zip::write::SimpleFileOptions::default(),
                )?;
                serde_json::to_writer(&mut writer, &self.manifest)?;
                writer.start_file(
                    project::HISTORY_ENTRY,
                    zip::write::SimpleFileOptions::default(),
                )?;
                serde_json::to_writer(&mut writer, &self.archived_history)?;
                names.insert(project::MANIFEST_ENTRY.to_owned());
                names.insert(project::HISTORY_ENTRY.to_owned());
            }
            for i in 0..archive.len() {
                let file = archive.by_index(i)?;
                if file.name() == source.stage() {
                    if !names.insert(target.stage().to_owned()) {
                        return Err("重复画布条目".into());
                    }
                    writer.start_file(
                        target.stage(),
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Deflated),
                    )?;
                    writer.write_all(&stage)?;
                } else if !file.name().starts_with(source.assets()) {
                    if source == Format::Mio
                        && matches!(
                            file.name(),
                            project::MANIFEST_ENTRY | project::HISTORY_ENTRY
                        )
                    {
                        continue;
                    }
                    let name = match (source, target) {
                        (Format::Prg, Format::Mio) => {
                            format!("{}{name}", project::LEGACY_PREFIX, name = file.name())
                        }
                        (Format::Mio, Format::Prg) => file
                            .name()
                            .strip_prefix(project::LEGACY_PREFIX)
                            .ok_or("项目包含 PRG 无法保留的扩展，请保存为 .mio")?
                            .to_owned(),
                        _ => file.name().to_owned(),
                    };
                    if target == Format::Mio {
                        project::validate_entry_name(&name)?;
                    }
                    if !names.insert(name.clone()) {
                        return Err(format!("重复项目条目：{name}").into());
                    }
                    writer.raw_copy_file_rename(file, name)?;
                }
            }
            for (name, bytes) in &self.attachments {
                let name = format!("{}{name}", target.assets());
                if target == Format::Mio {
                    project::validate_entry_name(&name)?;
                }
                if !names.insert(name.clone()) {
                    return Err(format!("重复项目条目：{name}").into());
                }
                writer.start_file(name, zip::write::SimpleFileOptions::default())?;
                writer.write_all(bytes)?;
            }
            writer.finish()?;
        }
        temp.as_file().sync_all()?;
        // Decode the actual output before atomically replacing any user file.
        let check = Graph::from_file(ProjectFile::load(temp.path())?)?;
        if !same_nodes(&check.nodes, &self.nodes) || !same_edges(&check, &self.graph) {
            return Err("保存往返校验不一致，原文件未修改".into());
        }
        let output_bytes = fs::read(temp.path())?;
        if target == Format::Mio {
            let contents =
                project::inspect(&mut zip::ZipArchive::new(Cursor::new(&output_bytes))?)?;
            if contents.manifest != self.manifest || contents.history != self.archived_history {
                return Err("项目清单或协作历史往返校验不一致，原文件未修改".into());
            }
        }
        if replace {
            temp.persist(destination)?;
        } else {
            temp.persist_noclobber(destination)?;
        }
        Ok(output_bytes)
    }
}
fn sync_container_kind(node: &mut GraphNode) {
    if node.kind == "TextNode" && !node.children.is_empty() {
        node.kind = "Section".into();
        node.auto_size = false;
    } else if node.kind == "Section" && node.children.is_empty() {
        node.kind = "TextNode".into();
        node.auto_size = true;
        node.collapsed = false;
        node.rect[0] += (node.rect[2] - 100.0) * 0.5;
        node.rect[1] += (node.rect[3] - 48.0) * 0.5;
        node.rect[2..].copy_from_slice(&[100.0, 48.0]);
    }
}

fn refreshed_group_bounds(nodes: &[GraphNode]) -> Result<Vec<[f32; 4]>> {
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut state = vec![0u8; nodes.len()];
    let mut bounds: Vec<_> = nodes.iter().map(|n| n.rect).collect();
    fn visit(
        index: usize,
        nodes: &[GraphNode],
        indices: &BTreeMap<&str, usize>,
        state: &mut [u8],
        bounds: &mut [[f32; 4]],
    ) -> Result<()> {
        if state[index] == 2 {
            return Ok(());
        }
        if state[index] == 1 {
            return Err("分组成员存在循环引用".into());
        }
        state[index] = 1;
        let node = &nodes[index];
        if node.kind == "Section" && !node.children.is_empty() {
            let mut members = vec![];
            for child in &node.children {
                let &child = indices.get(child.as_str()).ok_or("分组成员不存在")?;
                visit(child, nodes, indices, state, bounds)?;
                members.push(bounds[child]);
            }
            bounds[index] = group_bounds(members.into_iter(), !node.text.is_empty())?;
        }
        state[index] = 2;
        Ok(())
    }
    for index in 0..nodes.len() {
        visit(index, nodes, &indices, &mut state, &mut bounds)?;
    }
    Ok(bounds)
}

fn group_bounds(rects: impl Iterator<Item = [f32; 4]>, titled: bool) -> Result<[f32; 4]> {
    let mut lo = [f32::INFINITY; 2];
    let mut hi = [f32::NEG_INFINITY; 2];
    for r in rects {
        if !r.iter().all(|v| v.is_finite()) || r[2] <= 0.0 || r[3] <= 0.0 {
            return Err("分组成员边界无效".into());
        }
        for axis in 0..2 {
            lo[axis] = lo[axis].min(r[axis]);
            hi[axis] = hi[axis].max(r[axis] + r[axis + 2]);
        }
    }
    let title = if titled { 50.0 } else { 0.0 };
    let rect = [
        lo[0] - 30.0,
        lo[1] - 30.0 - title,
        (hi[0] - lo[0] + 60.0).max(100.0),
        hi[1] - lo[1] + 60.0 + title,
    ];
    if !rect.iter().all(|v| v.is_finite()) {
        return Err("分组成员边界无效".into());
    }
    Ok(rect)
}
fn same_nodes(a: &[GraphNode], b: &[GraphNode]) -> bool {
    let expected: BTreeMap<_, _> = b.iter().map(|n| (n.id.as_str(), n)).collect();
    a.len() == b.len()
        && expected.len() == b.len()
        && a.iter().all(|a| {
            let Some(&b) = expected.get(a.id.as_str()) else {
                return false;
            };
            // PRG's original serializer rounds coordinates to two decimal places.
            let geometry_matches = a
                .rect
                .iter()
                .zip(b.rect)
                .all(|(x, y)| (*x - y).abs() <= 0.01);
            let mut expected = b.clone();
            expected.rect = a.rect;
            let stroke_matches = a.stroke.len() == b.stroke.len()
                && a.stroke.iter().zip(&b.stroke).all(|(p, q)| {
                    (p[2] - q[2]).abs() <= 1e-6
                        && (0..2).all(|axis| {
                            let x = a.rect[axis] + p[axis] * a.rect[axis + 2];
                            let y = b.rect[axis] + q[axis] * b.rect[axis + 2];
                            (x - y).abs() <= 0.01
                        })
                });
            expected.stroke.clone_from(&a.stroke);
            geometry_matches && stroke_matches && a == &expected
        })
}
fn same_edges(actual: &Graph, expected: &Graph) -> bool {
    if actual.edges.len() != expected.edges.len() {
        return false;
    }
    let edges: BTreeMap<_, _> = expected.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    actual.edges.iter().all(|edge| {
        let Some(saved) = edges.get(edge.id.as_str()) else {
            return false;
        };
        let endpoints = edge
            .members
            .iter()
            .map(|&i| actual.nodes.get(i).map(|n| n.id.as_str()));
        let expected_endpoints = saved
            .members
            .iter()
            .map(|&i| expected.nodes.get(i).map(|n| n.id.as_str()));
        let color_matches = edge
            .color
            .iter()
            .zip(saved.color)
            .all(|(a, b)| (*a - b).abs() <= 1e-6);
        let mut normalized = (*saved).clone();
        normalized.members = edge.members.clone();
        normalized.color = edge.color;
        endpoints.eq(expected_endpoints) && color_matches && edge == &normalized
    })
}

fn append_nodes(stage: &mut Value, original: &[GraphNode], current: &[GraphNode]) -> Result<()> {
    let old: BTreeMap<_, _> = original.iter().map(|n| (n.id.as_str(), n)).collect();
    let live: BTreeMap<_, _> = current.iter().map(|n| (n.id.as_str(), n)).collect();
    if live.len() != current.len() {
        return Err("节点 ID 重复，原文件未修改".into());
    }
    let mut paths = BTreeMap::new();
    collect_object_paths(stage, "", &mut paths);
    let Value::Array(items) = stage else {
        return Err("stage.msgpack 必须是数组".into());
    };
    for node in current.iter().filter(|n| !old.contains_key(n.id.as_str())) {
        if paths.contains_key(&node.id) {
            return Err("新增节点类型或 ID 无效，原文件未修改".into());
        }
        items.push(match node.kind.as_str() {
            "Section" => section_value(node),
            "PenStroke" => pen_stroke_value(node),
            "TextNode" | "ConnectPoint" | "ImageNode" | "SvgNode" | "UrlNode" | "LatexNode" => {
                text_node_value(node)
            }
            _ => return Err("新增节点类型无效，原文件未修改".into()),
        });
    }
    let changed: Vec<_> = current
        .iter()
        .filter(|n| {
            (n.kind == "Section" || old.get(n.id.as_str()).is_some_and(|o| o.kind == "Section"))
                && old
                    .get(n.id.as_str())
                    .is_none_or(|o| o.children != n.children)
        })
        .collect();
    for node in &changed {
        if !old.contains_key(node.id.as_str()) {
            continue;
        }
        paths.clear();
        collect_object_paths(stage, "", &mut paths);
        let parent_path = paths.get(&node.id).ok_or("原分组缺少序列化路径")?;
        let object = object_mut(stage, &node.id).ok_or("原分组不存在")?;
        let Some(children) = field_mut(object, "children") else {
            continue;
        };
        let Value::Array(previous) = std::mem::replace(children, Value::Array(vec![])) else {
            return Err("原分组 children 不是数组".into());
        };
        let Value::Array(items) = stage else {
            unreachable!()
        };
        let mut relocated = Vec::new();
        for (index, child) in previous.into_iter().enumerate() {
            relocated.push((
                format!("{parent_path}/children/{index}"),
                format!("/{}", items.len()),
            ));
            items.push(child);
        }
        // Original serializer references point to object paths, including inline children.
        // Keep those objects and rewrite their paths before replacing membership arrays.
        relocate_references(stage, &relocated);
    }
    paths.clear();
    collect_object_paths(stage, "", &mut paths);
    for node in changed {
        let references = node
            .children
            .iter()
            .map(|id| {
                let path = paths.get(id).ok_or("分组成员不存在")?;
                Ok(Value::Map(vec![("$".into(), path.clone().into())]))
            })
            .collect::<Result<Vec<_>>>()?;
        let object = object_mut(stage, &node.id).ok_or("分组缺少序列化对象")?;
        put(object, "children", Value::Array(references));
    }
    Ok(())
}

fn object_mut<'a>(value: &'a mut Value, id: &str) -> Option<&'a mut Value> {
    if field(value, "uuid").and_then(Value::as_str) == Some(id) {
        return Some(value);
    }
    match value {
        Value::Array(items) => items.iter_mut().find_map(|v| object_mut(v, id)),
        Value::Map(items) => items.iter_mut().find_map(|(_, v)| object_mut(v, id)),
        _ => None,
    }
}

fn relocate_references(value: &mut Value, paths: &[(String, String)]) {
    if let Some(reference) = field_mut(value, "$")
        && let Some(path) = reference.as_str()
        && let Some((old, new)) = paths.iter().find(|(old, _)| {
            path == old
                || path
                    .strip_prefix(old)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    {
        *reference = format!("{new}{}", &path[old.len()..]).into();
    }
    match value {
        Value::Array(items) => {
            for item in items {
                relocate_references(item, paths);
            }
        }
        Value::Map(items) => {
            for (_, item) in items {
                relocate_references(item, paths);
            }
        }
        _ => {}
    }
}

fn pen_segments(node: &GraphNode) -> Value {
    Value::Array(
        node.stroke
            .iter()
            .map(|p| {
                Value::Map(vec![
                    ("_".into(), "PenStrokeSegment".into()),
                    (
                        "location".into(),
                        Value::Map(vec![
                            ("_".into(), "Vector".into()),
                            (
                                "x".into(),
                                Value::F64((node.rect[0] + p[0] * node.rect[2]) as f64),
                            ),
                            (
                                "y".into(),
                                Value::F64((node.rect[1] + p[1] * node.rect[3]) as f64),
                            ),
                        ]),
                    ),
                    ("pressure".into(), Value::F64(p[2] as f64)),
                ])
            })
            .collect(),
    )
}

fn pen_stroke_value(node: &GraphNode) -> Value {
    let common = text_node_value(node);
    Value::Map(vec![
        ("_".into(), "PenStroke".into()),
        ("uuid".into(), node.id.clone().into()),
        (
            "color".into(),
            field(&common, "color").cloned().unwrap_or(Value::Nil),
        ),
        ("locked".into(), node.locked.into()),
        ("segments".into(), pen_segments(node)),
    ])
}

fn text_node_value(node: &GraphNode) -> Value {
    let vector = |x: f32, y: f32| {
        Value::Map(vec![
            ("_".into(), "Vector".into()),
            ("x".into(), Value::F64(x as f64)),
            ("y".into(), Value::F64(y as f64)),
        ])
    };
    let [x, y, width, height] = node.rect;
    let mut value = Value::Map(vec![
        ("_".into(), node.kind.clone().into()),
        ("uuid".into(), node.id.clone().into()),
        ("text".into(), node.text.clone().into()),
        ("details".into(), Value::Array(vec![])),
        (
            "fontScaleLevel".into(),
            Value::F64(2.0 * (node.font_size as f64 / 32.0).log2()),
        ),
        ("fontFamily".into(), "".into()),
        ("fontSize".into(), Value::F64(node.font_size as f64)),
        ("fontWeight".into(), "".into()),
        (
            "sizeAdjust".into(),
            if node.auto_size { "auto" } else { "manual" }.into(),
        ),
        ("borderStyle".into(), "solid".into()),
        ("locked".into(), node.locked.into()),
        (
            "color".into(),
            Value::Map(vec![
                ("_".into(), "Color".into()),
                ("r".into(), Value::F64(node.color[0] as f64 * 255.0)),
                ("g".into(), Value::F64(node.color[1] as f64 * 255.0)),
                ("b".into(), Value::F64(node.color[2] as f64 * 255.0)),
                ("a".into(), Value::F64(node.color[3] as f64)),
            ]),
        ),
        (
            "collisionBox".into(),
            Value::Map(vec![
                ("_".into(), "CollisionBox".into()),
                (
                    "shapes".into(),
                    Value::Array(vec![Value::Map(vec![
                        ("_".into(), "Rectangle".into()),
                        ("location".into(), vector(x, y)),
                        ("size".into(), vector(width, height)),
                    ])]),
                ),
            ]),
        ),
    ]);
    if let Some(name) = &node.attachment {
        put(
            &mut value,
            "attachmentId",
            name.rsplit_once('.')
                .map_or(name.as_str(), |(id, _)| id)
                .into(),
        );
    }
    if let Some(name) = &node.source_name {
        put(&mut value, "grapluSourceName", name.clone().into());
    }
    if node.kind == "UrlNode" {
        put(&mut value, "title", node.text.clone().into());
    }
    if let Some(url) = &node.url {
        put(&mut value, "url", url.clone().into());
    }
    value
}

fn section_value(node: &GraphNode) -> Value {
    let vector = |x: f32, y: f32| {
        Value::Map(vec![
            ("_".into(), "Vector".into()),
            ("x".into(), Value::F64(x as f64)),
            ("y".into(), Value::F64(y as f64)),
        ])
    };
    let [x, y, w, h] = node.rect;
    let mut shapes: Vec<_> = [
        ([x, y], [x + w, y]),
        ([x + w, y], [x + w, y + h]),
        ([x + w, y + h], [x, y + h]),
        ([x, y + h], [x, y]),
    ]
    .into_iter()
    .map(|(a, b)| {
        Value::Map(vec![
            ("_".into(), "Line".into()),
            ("start".into(), vector(a[0], a[1])),
            ("end".into(), vector(b[0], b[1])),
        ])
    })
    .collect();
    if !node.text.is_empty() {
        shapes.push(Value::Map(vec![
            ("_".into(), "Rectangle".into()),
            ("location".into(), vector(x, y)),
            ("size".into(), vector(w, 50.0)),
        ]));
    }
    Value::Map(vec![
        ("_".into(), "Section".into()),
        ("uuid".into(), node.id.clone().into()),
        ("text".into(), node.text.clone().into()),
        ("children".into(), Value::Array(vec![])),
        ("locked".into(), node.locked.into()),
        ("isCollapsed".into(), node.collapsed.into()),
        ("borderStyle".into(), "solid".into()),
        (
            "color".into(),
            Value::Map(vec![
                ("_".into(), "Color".into()),
                ("r".into(), Value::F64(node.color[0] as f64 * 255.0)),
                ("g".into(), Value::F64(node.color[1] as f64 * 255.0)),
                ("b".into(), Value::F64(node.color[2] as f64 * 255.0)),
                ("a".into(), Value::F64(node.color[3] as f64)),
            ]),
        ),
        (
            "_collisionBoxNormal".into(),
            Value::Map(vec![
                ("_".into(), "CollisionBox".into()),
                ("shapes".into(), Value::Array(shapes)),
            ]),
        ),
    ])
}

fn append_edges(
    stage: &mut Value,
    original: &[GraphEdge],
    current: &[GraphEdge],
    original_nodes: &[GraphNode],
    nodes: &[GraphNode],
) -> Result<()> {
    let original_by_id: BTreeMap<_, _> = original.iter().map(|e| (e.id.as_str(), e)).collect();
    let current_by_id: BTreeMap<_, _> = current.iter().map(|e| (e.id.as_str(), e)).collect();
    if current_by_id.len() != current.len() {
        return Err("连线 ID 重复，原文件未修改".into());
    }
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut changed = Vec::new();
    for edge in current {
        if let Some(original) = original_by_id.get(edge.id.as_str()) {
            let mut expected = (*original).clone();
            expected.members = original
                .members
                .iter()
                .map(|&i| {
                    original_nodes
                        .get(i)
                        .and_then(|n| indices.get(n.id.as_str()))
                        .copied()
                        .ok_or_else(|| "原连线端点缺失".into())
                })
                .collect::<Result<Vec<_>>>()?;
            if &expected != edge {
                let mut normalized = edge.clone();
                normalized.text = expected.text.clone();
                normalized.members = expected.members.clone();
                normalized.rates = expected.rates.clone();
                if normalized != expected {
                    return Err("连线包含未支持的属性修改".into());
                }
                changed.push(edge);
            }
        }
    }
    if !changed.is_empty() {
        *stage = prune_deleted_stage(stage, &HashSet::new())?;
        let mut paths = BTreeMap::new();
        collect_object_paths(stage, "", &mut paths);
        for edge in changed {
            let members = edge
                .members
                .iter()
                .map(|&i| {
                    let node = nodes.get(i).ok_or("连线端点不存在")?;
                    let path = paths.get(&node.id).ok_or("连线端点缺失")?;
                    Ok(Value::Map(vec![("$".into(), path.clone().into())]))
                })
                .collect::<Result<Vec<_>>>()?;
            let vector = |r: [f32; 2]| {
                Value::Map(vec![
                    ("_".into(), "Vector".into()),
                    ("x".into(), Value::F64(r[0] as f64)),
                    ("y".into(), Value::F64(r[1] as f64)),
                ])
            };
            if edge.rates.len() < 2 {
                return Err("连线连接位置缺失".into());
            }
            let object = object_mut(stage, &edge.id).ok_or("连线不存在")?;
            put(object, "text", edge.text.clone().into());
            put(object, "associationList", Value::Array(members));
            put(object, "sourceRectangleRate", vector(edge.rates[0]));
            put(object, "targetRectangleRate", vector(edge.rates[1]));
            put(
                object,
                "rectRates",
                Value::Array(edge.rates.iter().copied().map(vector).collect()),
            );
        }
    }
    let mut paths = BTreeMap::new();
    collect_object_paths(stage, "", &mut paths);
    let Value::Array(items) = stage else {
        return Err("stage.msgpack 必须是数组".into());
    };
    for edge in current
        .iter()
        .filter(|e| !original_by_id.contains_key(e.id.as_str()))
    {
        if paths.contains_key(&edge.id)
            || edge.members.len() < 2
            || edge.rates.len() != edge.members.len()
            || !["LineEdge", "ArcEdge", "MultiTargetUndirectedEdge"].contains(&edge.kind.as_str())
            || (edge.kind != "MultiTargetUndirectedEdge" && edge.members.len() != 2)
        {
            return Err("新连线 ID 或端点无效，原文件未修改".into());
        }
        let references = edge
            .members
            .iter()
            .map(|&i| {
                let node = nodes.get(i).ok_or("连线端点不存在")?;
                let path = paths.get(&node.id).ok_or("连线端点缺少原始序列化引用")?;
                Ok(Value::Map(vec![("$".into(), path.clone().into())]))
            })
            .collect::<Result<Vec<_>>>()?;
        let vector = |rate: [f32; 2]| {
            Value::Map(vec![
                ("_".into(), "Vector".into()),
                ("x".into(), Value::F64(rate[0] as f64)),
                ("y".into(), Value::F64(rate[1] as f64)),
            ])
        };
        let color = Value::Map(vec![
            ("_".into(), "Color".into()),
            ("r".into(), Value::F64(edge.color[0] as f64 * 255.0)),
            ("g".into(), Value::F64(edge.color[1] as f64 * 255.0)),
            ("b".into(), Value::F64(edge.color[2] as f64 * 255.0)),
            ("a".into(), Value::F64(edge.color[3] as f64)),
        ]);
        items.push(Value::Map(vec![
            ("_".into(), edge.kind.clone().into()),
            ("uuid".into(), edge.id.clone().into()),
            ("associationList".into(), Value::Array(references)),
            ("sourceRectangleRate".into(), vector(edge.rates[0])),
            ("targetRectangleRate".into(), vector(edge.rates[1])),
            (
                "rectRates".into(),
                Value::Array(edge.rates.iter().copied().map(vector).collect()),
            ),
            ("renderType".into(), edge.render_type.clone().into()),
            ("arrow".into(), edge.multi_arrow.clone().into()),
            ("centerRate".into(), vector(edge.center_rate)),
            ("padding".into(), Value::F64(edge.padding as f64)),
            ("offset".into(), Value::F64(edge.arc_offset as f64)),
            ("textPosition".into(), Value::F64(edge.text_position as f64)),
            ("text".into(), edge.text.clone().into()),
            ("color".into(), color),
            ("lineType".into(), edge.line_type.clone().into()),
            (
                "arrowType".into(),
                if edge.arrow {
                    edge.arrow_type.clone().into()
                } else {
                    "none".into()
                },
            ),
        ]));
    }
    Ok(())
}

fn serialized_path<'a>(stage: &'a Value, path: &str) -> Result<&'a Value> {
    let mut value = stage;
    for part in path.split('/').filter(|part| !part.is_empty()) {
        value = match value {
            Value::Array(items) => part
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get(index)),
            Value::Map(_) => field(value, part),
            _ => None,
        }
        .ok_or_else(|| format!("PRG 引用路径不存在：{path}"))?;
    }
    Ok(value)
}

fn prune_deleted_stage(stage: &Value, deleted: &HashSet<&str>) -> Result<Value> {
    let mut paths = BTreeMap::new();
    collect_object_paths(stage, "", &mut paths);
    let mut removed: HashSet<String> = deleted.iter().map(|id| (*id).to_owned()).collect();
    for (id, path) in &paths {
        let object = serialized_path(stage, path)?;
        if field(object, "_").and_then(Value::as_str) == Some("SyncAssociation")
            && let Some(members) = field(object, "associationList").and_then(Value::as_array)
        {
            let mut count = 0;
            for member in members {
                let member = if let Some(path) = field(member, "$").and_then(Value::as_str) {
                    serialized_path(stage, path)?
                } else {
                    member
                };
                if field(member, "uuid")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !deleted.contains(id))
                {
                    count += 1;
                }
            }
            if count < 2 {
                removed.insert(id.clone());
            }
        }
    }
    // UUID-bearing objects may exist only inside a deleted edge or Section. Flatten
    // the surviving object graph first, then rewrite references using immutable IDs.
    let surviving: Vec<_> = paths
        .iter()
        .filter(|(id, _)| !removed.contains(*id))
        .collect();
    let canonical: BTreeMap<_, _> = surviving
        .iter()
        .enumerate()
        .map(|(index, (id, _))| ((*id).clone(), format!("/{index}")))
        .collect();
    fn convert(
        value: &Value,
        root: bool,
        stage: &Value,
        canonical: &BTreeMap<String, String>,
        active: &mut HashSet<String>,
    ) -> Result<Option<Value>> {
        if let Some(path) = field(value, "$").and_then(Value::as_str) {
            if !active.insert(path.to_owned()) {
                return Err("匿名 PRG 引用存在循环".into());
            }
            let result = convert(
                serialized_path(stage, path)?,
                false,
                stage,
                canonical,
                active,
            );
            active.remove(path);
            return result;
        }
        if !root && let Some(id) = field(value, "uuid").and_then(Value::as_str) {
            return Ok(canonical
                .get(id)
                .map(|path| Value::Map(vec![("$".into(), path.clone().into())])));
        }
        Ok(Some(match value {
            Value::Array(items) => {
                let mut output = vec![];
                for item in items {
                    if let Some(value) = convert(item, false, stage, canonical, active)? {
                        output.push(value);
                    }
                }
                Value::Array(output)
            }
            Value::Map(items) => {
                let mut output = vec![];
                for (key, item) in items {
                    output.push((
                        key.clone(),
                        convert(item, false, stage, canonical, active)?.unwrap_or(Value::Nil),
                    ));
                }
                Value::Map(output)
            }
            other => other.clone(),
        }))
    }
    let mut output = vec![];
    for (_, path) in surviving {
        let object = serialized_path(stage, path)?;
        output.push(
            convert(object, true, stage, &canonical, &mut HashSet::new())?.ok_or("存活对象缺失")?,
        );
    }
    // Preserve top-level extension metadata that does not represent a graph object.
    if let Value::Array(items) = stage {
        for item in items {
            if field(item, "uuid").is_none()
                && field(item, "$").is_none()
                && let Some(value) = convert(item, false, stage, &canonical, &mut HashSet::new())?
            {
                output.push(value);
            }
        }
    }
    Ok(Value::Array(output))
}

fn collect_object_paths(value: &Value, path: &str, paths: &mut BTreeMap<String, String>) {
    match value {
        Value::Map(items) => {
            if let Some(id) = field(value, "uuid").and_then(Value::as_str) {
                paths
                    .entry(id.to_owned())
                    .or_insert_with(|| path.to_owned());
            }
            for (key, item) in items {
                if let Some(key) = key.as_str() {
                    collect_object_paths(item, &format!("{path}/{key}"), paths);
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_object_paths(item, &format!("{path}/{index}"), paths);
            }
        }
        _ => {}
    }
}
fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value
        .as_map()?
        .iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .map(|(_, v)| v)
}
fn field_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    let Value::Map(map) = value else {
        return None;
    };
    map.iter_mut()
        .find(|(k, _)| k.as_str() == Some(key))
        .map(|(_, v)| v)
}
fn put(value: &mut Value, key: &str, item: Value) {
    if let Some(v) = field_mut(value, key) {
        *v = item;
    } else if let Value::Map(map) = value {
        map.push((key.into(), item));
    }
}
fn patch_stage(
    value: &mut Value,
    before: &BTreeMap<&str, &GraphNode>,
    after: &BTreeMap<&str, &GraphNode>,
) -> Result<()> {
    let id = field(value, "uuid")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(id) = id
        && let (Some(old), Some(new)) = (before.get(id.as_str()), after.get(id.as_str()))
    {
        let kind_changed = old.kind != new.kind;
        if kind_changed {
            let replacement = if new.kind == "Section" {
                section_value(new)
            } else {
                text_node_value(new)
            };
            put(value, "_", new.kind.clone().into());
            for key in ["collisionBox", "_collisionBoxNormal"] {
                if let Value::Map(fields) = value {
                    fields.retain(|(name, _)| name.as_str() != Some(key));
                }
                if let Some(shape) = field(&replacement, key) {
                    put(value, key, shape.clone());
                }
            }
            put(value, "fontSize", Value::F64(new.font_size as f64));
        }
        if old.text != new.text {
            put(value, "text", new.text.clone().into());
            if new.kind == "UrlNode" {
                put(value, "title", new.text.clone().into());
            }
        }
        if old.url != new.url {
            put(
                value,
                "url",
                new.url.clone().map_or(Value::Nil, Value::from),
            );
        }
        if old.font_size != new.font_size {
            put(value, "fontSize", Value::F64(new.font_size as f64));
            put(
                value,
                "fontScaleLevel",
                Value::F64(2.0 * (new.font_size as f64 / 32.0).log2()),
            );
        }
        if old.color != new.color {
            put(
                value,
                "color",
                Value::Map(vec![
                    ("_".into(), "Color".into()),
                    ("r".into(), Value::F64(new.color[0] as f64 * 255.0)),
                    ("g".into(), Value::F64(new.color[1] as f64 * 255.0)),
                    ("b".into(), Value::F64(new.color[2] as f64 * 255.0)),
                    ("a".into(), Value::F64(new.color[3] as f64)),
                ]),
            );
        }
        if (kind_changed || old.auto_size != new.auto_size) && new.kind == "TextNode" {
            put(
                value,
                "sizeAdjust",
                if new.auto_size { "auto" } else { "manual" }.into(),
            );
        }
        if old.locked != new.locked {
            put(value, "locked", new.locked.into());
        }
        if (kind_changed || old.collapsed != new.collapsed) && new.kind == "Section" {
            put(value, "isCollapsed", new.collapsed.into());
        }
        if new.kind == "PenStroke" && (old.rect != new.rect || old.stroke != new.stroke) {
            let replacement = pen_segments(new);
            // Retain extra segment fields when moving an imported stroke.
            match (field_mut(value, "segments"), replacement) {
                (Some(Value::Array(existing)), Value::Array(updated))
                    if existing.len() == updated.len() =>
                {
                    for (existing, updated) in existing.iter_mut().zip(updated) {
                        if field(existing, "$").is_some() {
                            return Err("暂不写入共享笔画引用，原文件未修改".into());
                        }
                        let location = field_mut(existing, "location").ok_or("笔画缺少坐标")?;
                        if field(location, "$").is_some() {
                            return Err("暂不写入共享笔画坐标，原文件未修改".into());
                        }
                        if let Some(updated_location) = field(&updated, "location") {
                            for axis in ["x", "y"] {
                                put(
                                    location,
                                    axis,
                                    field(updated_location, axis).ok_or("笔画坐标无效")?.clone(),
                                );
                            }
                        }
                        put(
                            existing,
                            "pressure",
                            field(&updated, "pressure").ok_or("笔画压力无效")?.clone(),
                        );
                    }
                }
                (_, replacement) => put(value, "segments", replacement),
            }
        } else if old.rect != new.rect && !kind_changed {
            let key = if field(value, "collisionBox").is_some() {
                "collisionBox"
            } else {
                "_collisionBoxNormal"
            };
            let shape = field_mut(value, key).ok_or("节点缺少碰撞形状")?;
            if field(shape, "$").is_some() {
                return Err("暂不写入共享碰撞形状引用，原文件未修改".into());
            }
            transform_shapes(shape, old.rect, new.rect)?;
        }
    }
    match value {
        Value::Array(items) => {
            for item in items {
                patch_stage(item, before, after)?;
            }
        }
        Value::Map(items) => {
            for (_, item) in items {
                patch_stage(item, before, after)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn transform_shapes(value: &mut Value, old: [f32; 4], new: [f32; 4]) -> Result<()> {
    let Some(Value::Array(shapes)) = field_mut(value, "shapes") else {
        return Err("碰撞形状无效".into());
    };
    for shape in shapes {
        let kind = field(shape, "_")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        for key in ["location", "start", "end"] {
            if let Some(point) = field_mut(shape, key) {
                for (axis, k) in ["x", "y"].iter().enumerate() {
                    let v = field(point, k).and_then(Value::as_f64).ok_or("坐标无效")? as f32;
                    let position = if old[axis + 2] == new[axis + 2] {
                        v + (new[axis] - old[axis])
                    } else {
                        new[axis] + (v - old[axis]) * new[axis + 2] / old[axis + 2]
                    };
                    put(point, k, Value::F64(position as f64));
                }
            }
        }
        if kind == "Rectangle"
            && let Some(size) = field_mut(shape, "size")
        {
            for (axis, k) in ["x", "y"].iter().enumerate() {
                let v = field(size, k).and_then(Value::as_f64).ok_or("尺寸无效")? as f32;
                put(
                    size,
                    k,
                    Value::F64((v * new[axis + 2] / old[axis + 2]) as f64),
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_edge_style_and_shape_survive_copy_save_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let doc = small_document(dir.path());
        let mut stage = ProjectFile::load(doc.path.as_ref().unwrap()).unwrap().stage;
        stage[0]["_"] = "ArcEdge".into();
        stage[0]["offset"] = (-123.5).into();
        stage[0]["textPosition"] = 0.3.into();
        stage[0]["lineType"] = "dashed".into();
        stage[0]["arrowType"] = "hollow-diamond".into();
        let mut doc = document_from_stage(dir.path(), &stage);
        assert_eq!(doc.edges[0].arc_offset, -123.5);
        assert_eq!(doc.edges[0].text_position, 0.3);
        doc.edit_edge(0, "弧线标题".into(), false).unwrap();
        let path = dir.path().join("arc.prg");
        doc.save(&path).unwrap();
        assert!(same_edges(&doc, &Document::open(&path).unwrap()));
        assert_eq!(
            ProjectFile::load(&path).unwrap().objects()["original-edge"]["unknown"]["keep"],
            serde_json::json!([1, 2, 3])
        );
        let mut clipboard = GraphClipboard::default();
        clipboard.copy(&doc, &HashSet::from([0, 1])).unwrap();
        let mut target = Document::blank().unwrap();
        clipboard.paste(&mut target, [500.0, 200.0]).unwrap();
        let path = dir.path().join("arc-copy.prg");
        target.save(&path).unwrap();
        assert!(same_edges(&target, &Document::open(&path).unwrap()));
    }

    #[test]
    fn pen_stroke_move_save_and_undo_preserve_pressure_and_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let stage = serde_json::json!([{"_":"PenStroke","uuid":"pen","extra":"keep","color":{"r":254,"g":154,"b":0,"a":1},"segments":[
            {"_":"PenStrokeSegment","location":{"_":"Vector","x":-5635.49,"y":5704.84,"extra":7},"pressure":0.2,"extra":"segment"},
            {"_":"PenStrokeSegment","location":{"_":"Vector","x":-5599.12,"y":5772.08},"pressure":0.8},
            {"_":"PenStrokeSegment","location":{"_":"Vector","x":-5614.2,"y":5731.99},"pressure":1.0}
        ]}]);
        let mut doc = document_from_stage(dir.path(), &stage);
        let before = doc.nodes.clone();
        doc.begin("移动涂鸦");
        doc.nodes[0].rect[0] += 24.0;
        doc.nodes[0].rect[1] -= 12.0;
        doc.finish();
        let moved = doc.nodes.clone();
        let path = dir.path().join("moved.prg");
        doc.save(&path).unwrap();
        let file = ProjectFile::load(&path).unwrap();
        let pen = file.objects()["pen"];
        assert_eq!(pen["extra"], "keep");
        assert_eq!(pen["segments"][0]["extra"], "segment");
        assert_eq!(pen["segments"][0]["location"]["extra"], 7);
        assert!((pen["segments"][0]["location"]["x"].as_f64().unwrap() - (-5611.49)).abs() < 0.01);
        let loaded = Document::open(&path).unwrap();
        assert!(same_nodes(&moved, &loaded.nodes));
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.save(&path).unwrap();
        assert!(same_nodes(&before, &Document::open(&path).unwrap().nodes));
        let mut copy = Document::blank().unwrap();
        let mut clipboard = GraphClipboard::default();
        clipboard.copy(&doc, &HashSet::from([0])).unwrap();
        clipboard.paste(&mut copy, [100.0, 200.0]).unwrap();
        let pasted = copy.nodes.clone();
        assert_eq!(pasted[0].stroke, before[0].stroke);
        assert_ne!(pasted[0].id, before[0].id);
        let copy_path = dir.path().join("copy.prg");
        copy.save(&copy_path).unwrap();
        assert!(same_nodes(
            &pasted,
            &Document::open(&copy_path).unwrap().nodes
        ));
    }

    #[test]
    fn url_destination_and_title_survive_save_copy_and_undo() {
        let dir = tempfile::tempdir().unwrap();
        let stage = serde_json::json!([{
            "_":"UrlNode", "uuid":"web", "title":"示例网站", "text":"旧文本字段", "url":"https://example.com/中文?q=1&x=2",
            "collisionBox":{"shapes":[{"_":"Rectangle", "location":{"x":0,"y":0}, "size":{"x":300,"y":150}}]}
        }]);
        let mut doc = document_from_stage(dir.path(), &stage);
        assert_eq!(doc.nodes[0].text, "示例网站");
        assert_eq!(
            doc.nodes[0].url.as_deref(),
            Some("https://example.com/中文?q=1&x=2")
        );
        let path = dir.path().join("url.prg");
        doc.save(&path).unwrap();
        assert!(same_nodes(
            &doc.nodes,
            &Document::open(&path).unwrap().nodes
        ));
        let mut clipboard = GraphClipboard::default();
        clipboard.copy(&doc, &HashSet::from([0])).unwrap();
        let mut copy = Document::blank().unwrap();
        clipboard.paste(&mut copy, [100.0; 2]).unwrap();
        assert_eq!(copy.nodes[0].url, doc.nodes[0].url);
        copy.save(&dir.path().join("url-copy.prg")).unwrap();
        let file = ProjectFile::load(&dir.path().join("url-copy.prg")).unwrap();
        assert_eq!(file.objects()[&copy.nodes[0].id]["title"], "示例网站");
        assert_eq!(
            file.objects()[&copy.nodes[0].id]["url"],
            "https://example.com/中文?q=1&x=2"
        );
        copy.undo();
        assert!(copy.nodes.is_empty());
        copy.redo();
        assert_eq!(copy.nodes[0].url, doc.nodes[0].url);
        copy.begin("更新网址");
        copy.nodes[0].url = Some("https://example.org/new".into());
        copy.finish();
        copy.save(&dir.path().join("url-copy.prg")).unwrap();
        assert_eq!(
            Document::open(&dir.path().join("url-copy.prg"))
                .unwrap()
                .nodes[0]
                .url
                .as_deref(),
            Some("https://example.org/new")
        );
    }

    #[test]
    fn section_title_edit_preserves_geometry_and_roundtrips_with_undo() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let group = doc.create_group(&HashSet::from([0])).unwrap();
        let original = doc.nodes[group].clone();
        assert!(doc.edit_text(&original.id, "新的分组名称".into(), [10.0, 10.0]));
        assert_eq!(doc.nodes[group].text, "新的分组名称");
        assert_eq!(doc.nodes[group].rect, original.rect);
        assert_eq!(doc.nodes[group].children, original.children);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("renamed.prg");
        doc.save(&path).unwrap();
        assert_eq!(
            Document::open(&path)
                .unwrap()
                .nodes
                .iter()
                .find(|node| node.id == original.id)
                .unwrap()
                .text,
            "新的分组名称"
        );
        doc.undo();
        assert_eq!(doc.nodes[group], original);
        doc.redo();
        assert_eq!(doc.nodes[group].text, "新的分组名称");
        doc.nodes[group].locked = true;
        assert!(!doc.edit_text(&original.id, "不可编辑".into(), [10.0; 2]));
        let child = doc.nodes[0].id.clone();
        assert!(!doc.edit_text(&child, "不可编辑".into(), [10.0; 2]));
    }

    #[test]
    fn batch_properties_preserve_untouched_fields_locks_and_exact_font_size_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        doc.nodes[1].locked = true;
        let before = doc.nodes.clone();
        let selected = HashSet::from([0, 1]);
        assert!(
            doc.set_node_properties(
                &selected,
                &NodeProperties {
                    color: Some([0.2, 0.4, 0.8, 1.0]),
                    font_size: Some(37.5),
                    width: Some(240.0),
                    auto_size: Some(false)
                }
            )
            .unwrap()
        );
        assert_eq!(doc.nodes[1], before[1]);
        assert_eq!(doc.nodes[0].font_size, 37.5);
        assert!(!doc.nodes[0].auto_size);
        assert_eq!(doc.nodes[0].text, before[0].text);
        let path = dir.path().join("properties.prg");
        doc.save(&path).unwrap();
        assert!(same_nodes(
            &doc.nodes,
            &Document::open(&path).unwrap().nodes
        ));
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.redo();
        assert_eq!(doc.nodes[0].rect[2], 240.0);
        let revision = doc.revision;
        assert!(
            doc.set_node_properties(
                &selected,
                &NodeProperties {
                    font_size: Some(f32::NAN),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(doc.revision, revision);
        assert_eq!(doc.search_nodes("NODE-A"), [0]);
        assert!(doc.search_nodes("  ").is_empty());
    }

    #[test]
    fn edge_title_and_direction_roundtrip_with_undo_after_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let original = doc.edges[0].clone();
        assert!(doc.edit_edge(0, "关系\n标题".into(), true).unwrap());
        assert_eq!(
            doc.edges[0].members,
            original.members.iter().copied().rev().collect::<Vec<_>>()
        );
        assert_eq!(
            doc.edges[0].rates,
            original.rates.iter().copied().rev().collect::<Vec<_>>()
        );
        let path = dir.path().join("edge-edit.prg");
        doc.save(&path).unwrap();
        assert!(same_edges(&doc, &Document::open(&path).unwrap()));
        doc.undo();
        assert_eq!(doc.edges[0], original);
        doc.save(&path).unwrap();
        assert!(same_edges(&doc, &Document::open(&path).unwrap()));
        doc.redo();
        doc.delete_objects(&HashSet::new(), &HashSet::from([0]))
            .unwrap();
        doc.save(&path).unwrap();
        assert!(Document::open(&path).unwrap().edges.is_empty());
        doc.undo();
        assert_eq!(doc.edges[0].text, "关系\n标题");
    }
    #[test]
    fn imported_images_keep_source_bytes_membership_and_single_step_history() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0, 0.0]).unwrap();
        let group = doc.create_group(&HashSet::from([0])).unwrap();
        let children = doc.nodes[group].children.clone();
        let bytes = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#.to_vec();
        let image = doc
            .import_image([0.0, 0.0], [20.0, 10.0], "原始图片.SVG", bytes.clone())
            .unwrap();
        let id = doc.nodes[image].id.clone();
        let name = doc.nodes[image].attachment.clone().unwrap();
        assert!(name.ends_with(".svg"));
        assert!(
            name.trim_end_matches(".svg")
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-')
        );
        assert_eq!(
            doc.nodes[image].source_name.as_deref(),
            Some("原始图片.SVG")
        );
        assert_eq!(doc.nodes[image].kind, "SvgNode");
        assert_eq!(doc.nodes[image].rect, [-10.0, -5.0, 20.0, 10.0]);
        assert!(doc.nodes[group].children.contains(&id));
        assert_eq!(doc.attachments[&name], bytes);
        doc.undo();
        assert!(!doc.nodes.iter().any(|node| node.id == id));
        assert_eq!(doc.nodes[group].children, children);
        doc.redo();
        assert!(doc.nodes[group].children.contains(&id));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("images.prg");
        doc.save(&path).unwrap();
        let restored = Document::open(&path).unwrap();
        assert_eq!(restored.attachments[&name], bytes);
        assert_eq!(
            restored
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .source_name
                .as_deref(),
            Some("原始图片.SVG")
        );
        assert!(
            restored
                .nodes
                .iter()
                .any(|node| node.id == id && node.attachment.as_ref() == Some(&name))
        );
        assert!(!doc.dirty());
        let second = doc
            .import_image([500.0, 0.0], [20.0, 10.0], "原始图片.SVG", bytes)
            .unwrap();
        assert_ne!(doc.nodes[second].attachment.as_ref(), Some(&name));
        doc.undo();
        assert!(!doc.dirty());
    }

    #[test]
    fn rejected_image_import_does_not_mutate_document_or_attachments() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0, 0.0]).unwrap();
        let group = doc.create_group(&HashSet::from([0])).unwrap();
        doc.nodes[group].locked = true;
        let before = doc.nodes.len();
        let revision = doc.revision;
        assert!(
            doc.import_image([0.0; 2], [20.0; 2], "a.png", vec![1])
                .is_err()
        );
        assert!(
            doc.import_image([500.0; 2], [20.0; 2], "../a.png", vec![1])
                .is_err()
        );
        assert!(
            doc.import_image([500.0; 2], [0.0; 2], "a.png", vec![1])
                .is_err()
        );
        assert!(
            doc.import_image([500.0; 2], [20.0; 2], "a.gif", vec![1])
                .is_err()
        );
        assert_eq!(doc.nodes.len(), before);
        assert_eq!(doc.revision, revision);
        assert!(doc.attachments.is_empty());
    }

    #[test]
    fn new_draft_can_save_edit_and_reopen_without_a_source_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("draft.prg");
        let mut doc = Document::blank().unwrap();
        doc.save(&path).unwrap();
        assert!(Document::open(&path).unwrap().nodes.is_empty());
        let index = doc.create_text_node([10.0, 20.0]).unwrap();
        let id = doc.nodes[index].id.clone();
        doc.edit_text(&id, "第一份想法".into(), [120.0, 40.0]);
        doc.save(&path).unwrap();
        assert_eq!(Document::open(&path).unwrap().nodes[0].text, "第一份想法");
        assert!(!doc.dirty());
    }
    fn small_document(dir: &Path) -> Document {
        let node = |id: &str| {
            serde_json::json!({
                "_": "TextNode", "uuid": id, "text": id,
            "sizeAdjust": if id == "node-b" { "manual" } else { "auto" },
                "collisionBox": {"_": "CollisionBox", "shapes": [{
                    "_": "Rectangle", "location": {"_": "Vector", "x": 0, "y": 0},
                    "size": {"_": "Vector", "x": 100, "y": 80}
                }]}
            })
        };
        let stage = serde_json::json!([
            {"_": "LineEdge", "uuid": "original-edge", "unknown": {"keep": [1, 2, 3]},
             "associationList": [node("node-b"), node("node-a")]},
            {"$": "/0/associationList/0"}, {"$": "/0/associationList/1"},
            {"_": "SyncAssociation", "uuid": "reserved-association"}
        ]);
        document_from_stage(dir, &stage)
    }
    fn document_from_stage(dir: &Path, stage: &serde_json::Value) -> Document {
        let path = dir.join("original.prg");
        let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        archive
            .start_file("stage.msgpack", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(&rmp_serde::to_vec_named(&stage).unwrap())
            .unwrap();
        archive
            .start_file("unknown.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&[0, 255, 4, 128]).unwrap();
        archive.finish().unwrap();
        Document::open(&path).unwrap()
    }
    fn section_document(dir: &Path) -> Document {
        let original = small_document(dir);
        let mut stage = ProjectFile::load(original.path.as_ref().unwrap())
            .unwrap()
            .stage;
        let children = stage[0]["associationList"].clone();
        stage[0]["associationList"] =
            serde_json::json!([{"$":"/4/children/0"}, {"$":"/4/children/1"}]);
        stage[1] = serde_json::json!({"$":"/4/children/0"});
        stage[2] = serde_json::json!({"$":"/4/children/1"});
        stage.as_array_mut().unwrap().push(serde_json::json!({
            "_":"Section", "uuid":"outer", "text":"原分组", "children":children,
            "unknown":{"preserve":true},
            "_collisionBoxNormal":{"_":"CollisionBox","shapes":[{"_":"Rectangle","location":{"x":-30,"y":-80},"size":{"x":160,"y":190}}]}
        }));
        document_from_stage(dir, &stage)
    }
    #[test]
    fn collapse_state_preserves_members_and_edges_through_save_undo_and_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let outer = doc
            .nodes
            .iter()
            .position(|node| node.id == "outer")
            .unwrap();
        let original = doc.nodes.clone();
        let edges = doc.edges.clone();
        let selection = HashSet::from([outer]);
        assert!(doc.set_sections_collapsed(&selection, true).unwrap());
        assert!(!doc.set_sections_collapsed(&selection, true).unwrap());
        assert!(!doc.section_overview_allowed("outer"));
        for (node, before) in doc.nodes.iter().zip(&original) {
            assert_eq!(node.rect, before.rect);
            assert_eq!(node.children, before.children);
        }
        assert_eq!(doc.edges, edges);
        let path = dir.path().join("collapsed.prg");
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert!(same_nodes(&doc.nodes, &reopened.nodes));
        assert!(same_edges(&doc, &reopened));
        for snapshot in [
            doc.capture_snapshot().unwrap(),
            doc.capture_selection_snapshot(&selection, &HashSet::new())
                .unwrap(),
        ] {
            assert!(!snapshot.section_overview_allowed("outer"));
            assert!(
                snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == "outer")
                    .unwrap()
                    .collapsed
            );
        }
        doc.undo();
        assert_eq!(doc.nodes, original);
        assert!(doc.dirty());
        assert!(doc.section_overview_allowed("outer"));
        doc.redo();
        assert!(doc.nodes[outer].collapsed);
        assert!(!doc.dirty());
        assert!(doc.set_sections_collapsed(&selection, false).unwrap());
        doc.save(&path).unwrap();
        assert!(
            Document::open(&path)
                .unwrap()
                .section_overview_allowed("outer")
        );
    }

    #[test]
    fn collapse_validation_is_atomic_and_keeps_active_edits() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let outer = doc
            .nodes
            .iter()
            .position(|node| node.id == "outer")
            .unwrap();
        let leaf = doc
            .nodes
            .iter()
            .position(|node| node.kind != "Section")
            .unwrap();
        let original = doc.nodes.clone();
        let revision = doc.revision;
        for selection in [
            HashSet::from([outer, leaf]),
            HashSet::from([outer, usize::MAX]),
        ] {
            assert!(doc.set_sections_collapsed(&selection, true).is_err());
            assert_eq!(doc.nodes, original);
            assert_eq!(doc.revision, revision);
        }
        doc.nodes[outer].locked = true;
        assert!(
            doc.set_sections_collapsed(&HashSet::from([outer]), true)
                .is_err()
        );
        doc.nodes[outer].locked = false;
        doc.begin("正在编辑");
        assert!(
            doc.set_sections_collapsed(&HashSet::from([outer]), true)
                .is_err()
        );
        assert!(doc.in_transaction());
        doc.cancel();
        assert_eq!(doc.nodes, original);
    }

    #[test]
    fn overview_respects_imported_section_border_and_collapse_state() {
        let dir = tempfile::tempdir().unwrap();
        let doc = section_document(dir.path());
        let mut stage = ProjectFile::load(doc.path.as_ref().unwrap()).unwrap().stage;
        for (style, collapsed, expected) in [
            ("solid", false, true),
            ("dashed", false, false),
            ("none", false, false),
            ("solid", true, false),
        ] {
            stage[4]["borderStyle"] = style.into();
            stage[4]["isCollapsed"] = collapsed.into();
            let loaded = document_from_stage(dir.path(), &stage);
            assert_eq!(loaded.section_overview_allowed("outer"), expected);
            assert!(!loaded.dirty());
        }
    }
    #[test]
    fn create_text_node_defaults_center_and_independent_undo_redo() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        doc.begin("先前移动");
        doc.nodes[0].rect[0] += 20.0;
        let before = doc.nodes.clone();
        let counts = doc.counts.clone();
        let index = doc.create_text_node([300.0, -200.0]).unwrap();
        let node = doc.nodes[index].clone();
        assert_eq!(index, before.len());
        assert_eq!(node.rect, [270.0, -228.5, 60.0, 57.0]);
        assert_eq!(node.kind, "TextNode");
        assert_eq!(node.text, "...");
        assert_eq!(node.font_size, 24.0);
        assert!(node.auto_size && !node.locked && node.children.is_empty());
        assert_eq!(node.color, [0.0; 4]);
        assert!(uuid::Uuid::parse_str(&node.id).is_ok());
        assert!(!doc.reserved_ids.contains(&node.id));
        assert!(doc.dirty());
        assert_eq!(doc.counts["TextNode"], counts["TextNode"] + 1);
        doc.undo();
        assert_eq!(doc.nodes, before);
        assert_eq!(doc.counts, counts);
        doc.redo();
        assert_eq!(doc.nodes[index], node);
        doc.undo();
        doc.undo();
        assert_eq!(doc.nodes[0].rect[0], 0.0);
        assert!(!doc.history.can_undo());
    }

    #[test]
    fn initial_text_layout_keeps_creation_center_and_survives_undo_redo() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let index = doc.create_text_node([300.0, -200.0]).unwrap();
        doc.undo();
        doc.redo();
        doc.save(&dir.path().join("before-layout.prg")).unwrap();
        let history_len = doc.history.len();
        assert!(doc.apply_layout_size(index, [100.0, 96.0]));
        assert_eq!(doc.nodes[index].rect, [250.0, -248.0, 100.0, 96.0]);
        assert!(!doc.dirty());
        assert_eq!(doc.history.len(), history_len);
        assert!(doc.pending_layouts.is_empty());
        let measured = doc.nodes[index].clone();
        for _ in 0..3 {
            doc.undo();
            assert_eq!(doc.nodes.len(), index);
            doc.redo();
            assert_eq!(doc.nodes[index], measured);
            assert!(!doc.apply_layout_size(index, [100.0, 96.0]));
        }
        doc.save(&dir.path().join("after-layout.prg")).unwrap();
        assert!(same_nodes(
            &Document::open(doc.path.as_ref().unwrap()).unwrap().nodes,
            &doc.nodes
        ));
    }

    #[test]
    fn initial_layout_after_early_drag_corrects_both_history_positions_without_snapback() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let index = doc.create_text_node([300.0, -200.0]).unwrap();
        doc.begin("首次测量前拖动");
        doc.nodes[index].rect[0] += 80.0;
        doc.nodes[index].rect[1] -= 20.0;
        assert!(!doc.apply_layout_size(index, [100.0, 96.0]));
        doc.finish();
        doc.undo();
        doc.redo();
        assert!(doc.apply_layout_size(index, [100.0, 96.0]));
        assert_eq!(doc.nodes[index].rect, [330.0, -268.0, 100.0, 96.0]);
        for _ in 0..3 {
            doc.undo();
            assert_eq!(doc.nodes[index].rect, [250.0, -248.0, 100.0, 96.0]);
            doc.redo();
            assert_eq!(doc.nodes[index].rect, [330.0, -268.0, 100.0, 96.0]);
        }
        doc.begin("测量后拖动");
        doc.nodes[index].rect[0] += 100.0;
        doc.finish();
        assert!(doc.apply_layout_size(index, [120.0, 116.0]));
        assert_eq!(doc.nodes[index].rect, [430.0, -268.0, 120.0, 116.0]);
        doc.undo();
        assert_eq!(doc.nodes[index].rect, [330.0, -268.0, 120.0, 116.0]);
        doc.redo();
        assert_eq!(doc.nodes[index].rect, [430.0, -268.0, 120.0, 116.0]);
    }

    #[test]
    fn first_layout_marker_never_centers_loaded_or_edited_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        assert!(doc.apply_layout_size(0, [200.0, 120.0]));
        assert_eq!(&doc.nodes[0].rect[..2], &[0.0, 0.0]);
        let index = doc.create_text_node([300.0, -200.0]).unwrap();
        let id = doc.nodes[index].id.clone();
        assert!(doc.edit_text(&id, "用户输入".into(), [140.0, 76.0]));
        assert!(doc.apply_layout_size(index, [160.0, 96.0]));
        assert_eq!(doc.nodes[index].rect, [270.0, -228.5, 160.0, 96.0]);
        // Undoing an edit made before first measurement restores the original
        // pending creation layout rather than consuming its center forever.
        doc.undo();
        assert!(doc.apply_layout_size(index, [100.0, 96.0]));
        assert_eq!(doc.nodes[index].rect, [250.0, -248.0, 100.0, 96.0]);
        doc.redo();
        assert_eq!(doc.nodes[index].text, "用户输入");
        assert_eq!(&doc.nodes[index].rect[..2], &[270.0, -228.5]);
    }

    #[test]
    fn create_text_node_joins_innermost_group_and_rejects_locked_ancestors() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let outer = doc.nodes.iter().position(|n| n.id == "outer").unwrap();
        let inner = doc.create_group(&HashSet::from([0])).unwrap();
        let before = doc.nodes.clone();
        let index = doc.create_text_node([50.0, 40.0]).unwrap();
        assert!(doc.nodes[inner].children.contains(&doc.nodes[index].id));
        assert!(!doc.nodes[outer].children.contains(&doc.nodes[index].id));
        let after = doc.nodes.clone();
        let path = dir.path().join("nested-create.prg");
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert!(same_nodes(&doc.nodes, &reopened.nodes));
        assert!(same_edges(&doc, &reopened));
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.redo();
        assert_eq!(doc.nodes, after);
        doc.nodes[outer].locked = true;
        let locked = doc.nodes.clone();
        let revision = doc.revision;
        assert!(doc.create_text_node([50.0, 40.0]).is_err());
        for center in [[f32::NAN, 0.0], [0.0, f32::INFINITY]] {
            assert!(doc.create_text_node(center).is_err());
        }
        assert_eq!(doc.nodes, locked);
        assert_eq!(doc.revision, revision);
    }

    #[test]
    fn creation_does_not_join_collapsed_or_hidden_containers() {
        for collapse_outer in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut doc = section_document(dir.path());
            let outer = doc.nodes.iter().position(|n| n.id == "outer").unwrap();
            let inner = doc.create_group(&HashSet::from([0])).unwrap();
            let collapsed = if collapse_outer { outer } else { inner };
            doc.set_sections_collapsed(&HashSet::from([collapsed]), true)
                .unwrap();
            let before = doc.nodes.clone();
            let index = doc.create_text_node([50.0, 40.0]).unwrap();
            let id = doc.nodes[index].id.clone();
            assert!(!doc.nodes[inner].children.contains(&id));
            assert_eq!(doc.nodes[outer].children.contains(&id), !collapse_outer);
            assert_eq!(doc.section_representatives().unwrap()[index], index);
            let after = doc.nodes.clone();
            let path = dir.path().join("collapsed-create.prg");
            doc.save(&path).unwrap();
            let reopened = Document::open(&path).unwrap();
            assert!(same_nodes(&doc.nodes, &reopened.nodes));
            doc.undo();
            assert_eq!(doc.nodes, before);
            doc.redo();
            assert_eq!(doc.nodes, after);
        }
    }

    #[test]
    fn new_text_node_edit_group_link_delete_save_and_undo_preserve_original_archive() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let original_stage = ProjectFile::load(doc.path.as_ref().unwrap()).unwrap().stage;
        let original_nodes = doc.nodes.clone();
        let original_edges = doc.edges.clone();
        let path = dir.path().join("new-text.prg");
        let index = doc.create_text_node([500.0, 400.0]).unwrap();
        let id = doc.nodes[index].id.clone();
        doc.save(&path).unwrap();
        let file = ProjectFile::load(&path).unwrap();
        let objects = file.objects();
        let object = objects[&id];
        for (key, value) in [
            ("sizeAdjust", serde_json::json!("auto")),
            ("fontSize", serde_json::json!(24.0)),
            ("fontFamily", serde_json::json!("")),
            ("fontWeight", serde_json::json!("")),
            ("borderStyle", serde_json::json!("solid")),
            ("details", serde_json::json!([])),
        ] {
            assert_eq!(object[key], value, "{key}");
        }
        assert!(doc.edit_text(&id, "新增中文\n第二行".into(), [220.0, 124.0]));
        let group = doc.create_group(&HashSet::from([index])).unwrap();
        doc.add_edge(GraphEdge {
            id: "new-text-edge".into(),
            members: vec![0, index],
            rates: vec![[0.5; 2]; 2],
            ..Default::default()
        })
        .unwrap();
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert!(same_nodes(&doc.nodes, &reopened.nodes));
        assert!(same_edges(&doc, &reopened));
        assert!(doc.nodes[group].children.contains(&id));
        doc.delete_objects(&HashSet::from([index]), &HashSet::new())
            .unwrap();
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert!(same_nodes(&doc.nodes, &reopened.nodes));
        assert!(same_edges(&doc, &reopened));
        assert!(!reopened.nodes.iter().any(|n| n.id == id));
        assert!(!reopened.edges.iter().any(|e| e.id == "new-text-edge"));
        for _ in 0..5 {
            doc.undo();
            doc.save(&path).unwrap();
            let reopened = Document::open(&path).unwrap();
            assert!(same_nodes(&doc.nodes, &reopened.nodes));
            assert!(same_edges(&doc, &reopened));
        }
        assert_eq!(doc.nodes, original_nodes);
        assert_eq!(doc.edges, original_edges);
        assert_eq!(ProjectFile::load(&path).unwrap().stage, original_stage);
        assert!(!doc.history.can_undo());
        let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let mut unknown = vec![];
        archive
            .by_name("unknown.bin")
            .unwrap()
            .read_to_end(&mut unknown)
            .unwrap();
        assert_eq!(unknown, [0, 255, 4, 128]);
        doc.redo();
        assert_eq!(doc.nodes[index].id, id);
        doc.save(&path).unwrap();
        assert!(same_nodes(
            &Document::open(&path).unwrap().nodes,
            &doc.nodes
        ));
        // Removing an original inline object invokes reference pruning; newly
        // serialized TextNodes must also survive that path and index remapping.
        doc.delete_objects(&HashSet::from([0]), &HashSet::new())
            .unwrap();
        let second = doc.create_text_node([-500.0, -400.0]).unwrap();
        assert_ne!(doc.nodes[second].id, id);
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert!(same_nodes(&doc.nodes, &reopened.nodes));
        assert!(same_edges(&doc, &reopened));
    }
    #[test]
    fn jumping_into_text_and_emptying_it_converts_with_history_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let before = doc.nodes.clone();
        let target_id = doc.nodes[1].id.clone();
        let source_id = doc.nodes[0].id.clone();
        let title = doc.nodes[1].text.clone();
        let plan = doc
            .jump_plan(&HashSet::from([0]), Some(1), [400.0, 300.0])
            .unwrap();
        assert_eq!(doc.nodes, before);
        assert_eq!(plan.nodes[1].kind, "Section");
        doc.jump_selection(&HashSet::from([0]), Some(1), [400.0, 300.0])
            .unwrap();
        assert_eq!(doc.nodes[1].kind, "Section");
        assert_eq!(doc.nodes[1].text, title);
        assert_eq!(
            doc.nodes[1].children.as_slice(),
            std::slice::from_ref(&source_id)
        );
        let grouped = doc.nodes.clone();
        let path = dir.path().join("automatic-group.prg");
        doc.save(&path).unwrap();
        let mut reopened = Document::open(&path).unwrap();
        assert!(same_nodes(&reopened.nodes, &grouped));
        assert!(same_edges(&doc, &reopened));
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.redo();
        assert_eq!(doc.nodes, grouped);
        let source = reopened
            .nodes
            .iter()
            .position(|n| n.id == source_id)
            .unwrap();
        let target = reopened
            .nodes
            .iter()
            .position(|n| n.id == target_id)
            .unwrap();
        reopened
            .jump_selection(&HashSet::from([source]), None, [800.0, 300.0])
            .unwrap();
        assert_eq!(reopened.nodes[target].kind, "TextNode");
        assert!(reopened.nodes[target].auto_size);
        assert!(reopened.nodes[target].children.is_empty());
        assert_eq!(reopened.nodes[target].text, title);
        reopened.save(&path).unwrap();
        assert!(same_nodes(
            &reopened.nodes,
            &Document::open(&path).unwrap().nodes
        ));
        reopened.undo();
        assert!(same_nodes(&reopened.nodes, &grouped));
        reopened.redo();
        assert_eq!(reopened.nodes[target].kind, "TextNode");
    }

    #[test]
    fn deleting_last_child_turns_parent_into_text_and_undo_restores_group() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0, 0.0]).unwrap();
        let group = doc.create_group(&HashSet::from([0])).unwrap();
        let id = doc.nodes[group].id.clone();
        let before = doc.nodes.clone();
        doc.delete_objects(&HashSet::from([0]), &HashSet::new())
            .unwrap();
        let parent = doc.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(parent.kind, "TextNode");
        assert!(parent.children.is_empty());
        doc.undo();
        assert_eq!(doc.nodes, before);
    }

    #[test]
    fn jump_moves_nested_selection_once_and_reparents_with_one_undo() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let outer = doc.nodes.iter().position(|n| n.id == "outer").unwrap();
        let group = doc.create_group(&HashSet::from([0])).unwrap();
        let before = doc.nodes.clone();
        let edges = doc.edges.clone();
        let selection = HashSet::from([group, 0]);
        let plan = doc.jump_plan(&selection, None, [800.0, -300.0]).unwrap();
        assert_eq!(plan.roots, [group]);
        assert_eq!(plan.moving, selection);
        assert_eq!(doc.nodes, before, "preview must not mutate the document");
        assert_eq!(
            plan.destination_bounds[0] + plan.destination_bounds[2] * 0.5,
            800.0
        );
        assert_eq!(
            plan.destination_bounds[1] + plan.destination_bounds[3] * 0.5,
            -300.0
        );
        doc.jump_selection(&selection, None, [800.0, -300.0])
            .unwrap();
        assert_eq!(doc.nodes[outer].children, ["node-b"]);
        assert_eq!(doc.nodes[group].children, ["node-a"]);
        for &index in &selection {
            for axis in 0..2 {
                assert!(
                    (doc.nodes[index].rect[axis] - before[index].rect[axis] - plan.delta[axis])
                        .abs()
                        < 0.01
                );
            }
        }
        let jumped = doc.nodes.clone();
        assert_eq!(doc.edges, edges);
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.redo();
        assert_eq!(doc.nodes, jumped);
        doc.jump_selection(&selection, Some(outer), [100.0, 200.0])
            .unwrap();
        assert_eq!(
            doc.nodes[outer].children,
            ["node-b", doc.nodes[group].id.as_str()]
        );
        doc.undo();
        assert_eq!(doc.nodes, jumped);
    }
    #[test]
    fn jump_rejects_invalid_targets_and_locks_without_touching_pending_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let outer = doc.nodes.iter().position(|n| n.id == "outer").unwrap();
        let nested = doc.create_group(&HashSet::from([0])).unwrap();
        let revision = doc.revision;
        let before = doc.nodes.clone();
        for (selection, destination, point) in [
            (HashSet::new(), None, [0.0, 0.0]),
            (HashSet::from([99]), None, [0.0, 0.0]),
            (HashSet::from([0]), Some(99), [0.0, 0.0]),
            (HashSet::from([outer]), Some(outer), [0.0, 0.0]),
            (HashSet::from([outer]), Some(nested), [0.0, 0.0]),
            (HashSet::from([0]), None, [f32::NAN, 0.0]),
            (HashSet::from([0]), None, [0.0, f32::INFINITY]),
        ] {
            assert!(doc.jump_selection(&selection, destination, point).is_err());
            assert_eq!(doc.nodes, before);
            assert_eq!(doc.revision, revision);
        }
        for index in [0, outer, nested] {
            doc.nodes[index].locked = true;
            assert!(
                doc.jump_plan(&HashSet::from([0]), None, [1.0, 2.0])
                    .is_err()
            );
            assert!(
                doc.jump_plan(&HashSet::from([outer]), None, [1.0, 2.0])
                    .is_err()
            );
            doc.nodes[index].locked = false;
        }
        doc.nodes[nested].locked = true;
        assert!(
            doc.jump_plan(&HashSet::from([1]), Some(nested), [0.0, 0.0])
                .is_err()
        );
        doc.nodes[nested].locked = false;
        doc.begin("原有拖动");
        doc.nodes[0].rect[0] += 20.0;
        let dragged = doc.nodes.clone();
        assert!(
            doc.jump_selection(&HashSet::from([outer]), Some(nested), [0.0, 0.0])
                .is_err()
        );
        assert!(doc.in_transaction());
        assert_eq!(doc.nodes, dragged);
        doc.jump_selection(&HashSet::from([0]), None, [600.0, 500.0])
            .unwrap();
        assert!(!doc.in_transaction());
        doc.undo();
        assert_eq!(
            doc.nodes, before,
            "drag and jump must undo as one transaction"
        );
    }
    #[test]
    fn jump_save_roundtrip_preserves_edges_unknown_entries_and_undo() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let group = doc.create_group(&HashSet::from([1])).unwrap();
        let baseline = dir.path().join("before-jump.prg");
        doc.save(&baseline).unwrap();
        let before = doc.nodes.clone();
        let edges = doc.edges.clone();
        doc.jump_selection(&HashSet::from([0]), Some(group), [300.0, 400.0])
            .unwrap();
        let output = dir.path().join("jumped.prg");
        for _ in 0..2 {
            doc.save(&output).unwrap();
            let saved = Document::open(&output).unwrap();
            assert!(same_nodes(&saved.nodes, &doc.nodes));
            assert!(same_edges(&saved, &doc));
            let file = ProjectFile::load(&output).unwrap();
            assert_eq!(file.objects()["outer"]["unknown"]["preserve"], true);
            let mut archive = zip::ZipArchive::new(fs::File::open(&output).unwrap()).unwrap();
            let mut bytes = vec![];
            archive
                .by_name("unknown.bin")
                .unwrap()
                .read_to_end(&mut bytes)
                .unwrap();
            assert_eq!(bytes, [0, 255, 4, 128]);
        }
        assert_eq!(doc.edges, edges);
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.save(&output).unwrap();
        assert_eq!(
            ProjectFile::load(&output).unwrap().stage,
            ProjectFile::load(&baseline).unwrap().stage
        );
        doc.redo();
        doc.jump_selection(&HashSet::from([0]), None, [-300.0, -400.0])
            .unwrap();
        doc.save(&output).unwrap();
        let saved = Document::open(&output).unwrap();
        assert!(same_nodes(&saved.nodes, &doc.nodes));
        assert!(same_edges(&saved, &doc));
        assert!(
            !saved
                .nodes
                .iter()
                .any(|n| n.children.contains(&"node-a".into()))
        );
    }
    #[test]
    fn grouping_reparents_nested_members_and_undo_restores_them_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let before = doc.nodes.clone();
        let outer = doc.nodes.iter().position(|n| n.id == "outer").unwrap();
        let group = doc.create_group(&HashSet::from([0])).unwrap();
        assert_eq!(group, 3);
        let group_id = doc.nodes[group].id.clone();
        assert_eq!(doc.nodes[group].children, ["node-a"]);
        assert_eq!(doc.nodes[outer].children, ["node-b", group_id.as_str()]);
        assert_eq!(doc.nodes[group].rect, [-30.0, -80.0, 160.0, 190.0]);
        let grouped = doc.nodes.clone();
        doc.undo();
        assert_eq!(doc.nodes, before);
        assert!(!doc.dirty());
        doc.redo();
        assert_eq!(doc.nodes, grouped);
        let parent = doc.create_group(&HashSet::from([0, outer, group])).unwrap();
        assert_eq!(doc.nodes[parent].children, ["outer"]);
        assert_eq!(
            doc.expand_selection(&HashSet::from([parent])),
            HashSet::from([0, 1, 2, 3, 4])
        );
        doc.undo();
        assert_eq!(doc.nodes, grouped);
        doc.nodes[outer].locked = true;
        assert!(doc.create_group(&HashSet::from([0])).is_err());
        assert!(doc.create_group(&HashSet::new()).is_err());
        assert!(doc.create_group(&HashSet::from([99])).is_err());
    }
    #[test]
    fn group_save_relocates_inline_members_preserving_edges_unknowns_and_repeated_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let original = ProjectFile::load(doc.path.as_ref().unwrap()).unwrap().stage;
        let group = doc.create_group(&HashSet::from([0])).unwrap();
        let group_id = doc.nodes[group].id.clone();
        let output = dir.path().join("grouped.prg");
        for _ in 0..2 {
            doc.save(&output).unwrap();
            let saved = ProjectFile::load(&output).unwrap();
            assert_eq!(saved.stage[4]["unknown"], original[4]["unknown"]);
            assert_eq!(saved.stage[0]["unknown"], original[0]["unknown"]);
            let graph = Graph::from_file(saved).unwrap();
            assert!(same_nodes(&graph.nodes, &doc.nodes));
            assert!(same_edges(&graph, &doc));
            assert_eq!(graph.nodes.iter().filter(|n| n.id == group_id).count(), 1);
        }
        doc.undo();
        doc.save(&output).unwrap();
        assert_eq!(ProjectFile::load(&output).unwrap().stage, original);
        doc.redo();
        doc.add_edge(GraphEdge {
            id: "group-edge".into(),
            members: vec![group, 1],
            rates: vec![[0.5; 2]; 2],
            ..GraphEdge::default()
        })
        .unwrap();
        doc.save(&output).unwrap();
        let loaded = Document::open(&output).unwrap();
        assert!(same_nodes(&loaded.nodes, &doc.nodes));
        assert!(same_edges(&loaded, &doc));
        let mut archive = zip::ZipArchive::new(fs::File::open(output).unwrap()).unwrap();
        let mut bytes = vec![];
        archive
            .by_name("unknown.bin")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, [0, 255, 4, 128]);
    }
    #[test]
    fn reopening_saved_groups_allows_nested_grouping_and_lossless_undo_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut initial = small_document(dir.path());
        initial.create_group(&HashSet::from([0, 1])).unwrap();
        let output = dir.path().join("grouped.prg");
        initial.save(&output).unwrap();
        let before = ProjectFile::load(&output).unwrap().stage;
        let mut reopened = Document::open(&output).unwrap();
        let child = reopened
            .nodes
            .iter()
            .position(|n| n.id == "node-a")
            .unwrap();
        let nested = reopened.create_group(&HashSet::from([child])).unwrap();
        let nested_id = reopened.nodes[nested].id.clone();
        reopened.save(&output).unwrap();
        let saved = Document::open(&output).unwrap();
        assert!(same_nodes(&saved.nodes, &reopened.nodes));
        assert!(same_edges(&saved, &reopened));
        assert!(saved.nodes.iter().any(|n| n.children.contains(&nested_id)));
        reopened.undo();
        reopened.save(&output).unwrap();
        assert_eq!(ProjectFile::load(&output).unwrap().stage, before);
    }
    #[test]
    fn group_bounds_follow_child_moves_with_one_undo_entry() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let group = doc.create_group(&HashSet::from([0, 1])).unwrap();
        let before = doc.nodes.clone();
        doc.begin("move child");
        doc.nodes[0].rect[0] += 300.0;
        assert!(doc.refresh_group_bounds().unwrap());
        doc.finish();
        assert_eq!(doc.nodes[group].rect, [-30.0, -80.0, 460.0, 190.0]);
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.undo();
        assert_eq!(doc.nodes.len(), 2);
        assert!(!doc.dirty());
    }
    fn new_edge() -> GraphEdge {
        GraphEdge {
            id: "added-edge".into(),
            members: vec![0, 1],
            rates: vec![[0.99, 0.5], [0.01, 0.5]],
            text: "中文关系".into(),
            line_type: "dashed".into(),
            color: [0.23, 0.47, 0.79, 0.8],
            ..GraphEdge::default()
        }
    }
    #[test]
    fn cutting_an_edge_preserves_its_inline_nodes_and_undo_restores_the_original_stage() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let original = ProjectFile::load(doc.path.as_ref().unwrap()).unwrap().stage;
        let before = doc.nodes.clone();
        doc.delete_objects(&HashSet::new(), &HashSet::from([0]))
            .unwrap();
        assert!(doc.edges.is_empty());
        assert_eq!(doc.nodes, before);
        let output = dir.path().join("cut-edge.prg");
        doc.save(&output).unwrap();
        let loaded = Document::open(&output).unwrap();
        assert_eq!(loaded.nodes, before);
        assert!(loaded.edges.is_empty());
        doc.undo();
        assert_eq!(doc.edges.len(), 1);
        doc.save(&output).unwrap();
        assert_eq!(ProjectFile::load(&output).unwrap().stage, original);
        doc.redo();
        doc.save(&output).unwrap();
        assert!(Document::open(&output).unwrap().edges.is_empty());
    }
    #[test]
    fn cutting_a_node_remaps_surviving_edges_and_preserves_shared_values() {
        let dir = tempfile::tempdir().unwrap();
        let initial = small_document(dir.path());
        let mut stage = ProjectFile::load(initial.path.as_ref().unwrap())
            .unwrap()
            .stage;
        let mut third = stage[0]["associationList"][1].clone();
        third["uuid"] = "node-c".into();
        third["text"] = "surviving third node".into();
        third["unknown"] =
            serde_json::json!({"$":"/0/associationList/1/collisionBox/shapes/0/location"});
        stage.as_array_mut().unwrap().push(third);
        stage.as_array_mut().unwrap().push(serde_json::json!({
            "_":"LineEdge", "uuid":"surviving-edge", "text":"keep", "associationList":[{"$":"/0/associationList/0"},{"$":"/4"}],
            "unknown":{"binaryMetadata":[0,255,4]}
        }));
        let mut doc = document_from_stage(dir.path(), &stage);
        let counts = doc.counts.clone();
        doc.delete_objects(&HashSet::from([0]), &HashSet::new())
            .unwrap();
        assert_eq!(
            doc.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["node-b", "node-c"]
        );
        assert_eq!(doc.edges.len(), 1);
        assert_eq!(doc.edges[0].members, [0, 1]);
        assert_eq!(doc.counts["TextNode"], counts["TextNode"] - 1);
        assert_eq!(doc.counts["LineEdge"], counts["LineEdge"] - 1);
        let output = dir.path().join("cut-node.prg");
        doc.save(&output).unwrap();
        let saved = ProjectFile::load(&output).unwrap();
        let objects = saved.objects();
        assert_eq!(
            objects["node-c"]["unknown"],
            stage[0]["associationList"][1]["collisionBox"]["shapes"][0]["location"]
        );
        assert_eq!(objects["surviving-edge"]["unknown"], stage[5]["unknown"]);
        let loaded = Document::open(&output).unwrap();
        assert!(same_nodes(&loaded.nodes, &doc.nodes));
        assert!(same_edges(&loaded, &doc));
        doc.undo();
        assert_eq!(doc.counts, counts);
        doc.save(&output).unwrap();
        assert_eq!(ProjectFile::load(&output).unwrap().stage, stage);
    }
    #[test]
    fn cutting_sections_promotes_expanded_members_but_deletes_collapsed_members() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = section_document(dir.path());
        let before = doc.nodes.clone();
        let outer = doc.nodes.iter().position(|n| n.id == "outer").unwrap();
        let nested = doc.create_group(&HashSet::from([0])).unwrap();
        doc.delete_objects(&HashSet::from([nested]), &HashSet::new())
            .unwrap();
        assert_eq!(
            doc.nodes[outer]
                .children
                .iter()
                .cloned()
                .collect::<HashSet<_>>(),
            HashSet::from(["node-a".into(), "node-b".into()])
        );
        doc.undo();
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.delete_objects(&HashSet::from([outer]), &HashSet::new())
            .unwrap();
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.edges.len(), 1);
        let output = dir.path().join("ungrouped.prg");
        doc.save(&output).unwrap();
        let loaded = Document::open(&output).unwrap();
        assert!(same_nodes(&loaded.nodes, &doc.nodes));
        assert!(same_edges(&loaded, &doc));
        doc.undo();
        let mut stage = ProjectFile::load(&dir.path().join("original.prg"))
            .unwrap()
            .stage;
        stage[4]["isCollapsed"] = true.into();
        let mut collapsed = document_from_stage(dir.path(), &stage);
        let outer = collapsed
            .nodes
            .iter()
            .position(|n| n.id == "outer")
            .unwrap();
        collapsed
            .delete_objects(&HashSet::from([outer]), &HashSet::new())
            .unwrap();
        assert!(collapsed.nodes.is_empty() && collapsed.edges.is_empty());
        collapsed.save(&output).unwrap();
        assert!(Document::open(&output).unwrap().nodes.is_empty());
        collapsed.undo();
        collapsed.save(&output).unwrap();
        assert_eq!(ProjectFile::load(&output).unwrap().stage, stage);
    }
    #[test]
    fn cutting_respects_locks_and_only_removes_affected_isolated_connection_points() {
        let dir = tempfile::tempdir().unwrap();
        let mut graph = small_document(dir.path()).graph;
        graph.nodes[0].kind = "ConnectPoint".into();
        let mut unrelated = graph.nodes[0].clone();
        unrelated.id = "unrelated-isolated-point".into();
        graph.nodes.push(unrelated);
        let mut doc = Document::memory(graph);
        doc.nodes[1].locked = true;
        let plan = doc
            .deletion_plan(&HashSet::from([1]), &HashSet::from([0]))
            .unwrap();
        assert!(plan.nodes.is_empty() && plan.edges.is_empty());
        doc.delete_objects(&plan.nodes, &plan.edges).unwrap();
        assert!(!doc.dirty());
        doc.nodes[1].locked = false;
        let plan = doc
            .deletion_plan(&HashSet::new(), &HashSet::from([0]))
            .unwrap();
        assert_eq!(plan.nodes, HashSet::from([0]));
        doc.delete_objects(&HashSet::new(), &HashSet::from([0]))
            .unwrap();
        assert_eq!(
            doc.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["node-b", "unrelated-isolated-point"]
        );
        doc.undo();
        assert_eq!(doc.nodes.len(), 3);
        let mut grouped = section_document(dir.path());
        grouped
            .nodes
            .iter_mut()
            .find(|n| n.id == "outer")
            .unwrap()
            .locked = true;
        assert_eq!(grouped.protected_nodes(), HashSet::from([0, 1, 2]));
        grouped
            .delete_objects(&HashSet::from([0, 1, 2]), &HashSet::from([0]))
            .unwrap();
        assert!(!grouped.dirty());
        assert_eq!(grouped.nodes.len(), 3);
    }
    #[test]
    fn deletion_normalization_keeps_messagepack_extensions_and_clears_short_sync_relations() {
        let mut stage = Value::Array(vec![
            Value::Map(vec![
                ("_".into(), "TextNode".into()),
                ("uuid".into(), "delete".into()),
            ]),
            Value::Map(vec![
                ("_".into(), "TextNode".into()),
                ("uuid".into(), "keep".into()),
                ("unknown".into(), Value::Ext(4, vec![0, 255, 1])),
            ]),
            Value::Map(vec![
                ("_".into(), "SyncAssociation".into()),
                ("uuid".into(), "sync".into()),
                (
                    "associationList".into(),
                    Value::Array(vec![
                        Value::Map(vec![("$".into(), "/0".into())]),
                        Value::Map(vec![("$".into(), "/1".into())]),
                    ]),
                ),
            ]),
            Value::Map(vec![("metadata".into(), Value::Binary(vec![3, 4]))]),
        ]);
        stage = prune_deleted_stage(&stage, &HashSet::from(["delete"])).unwrap();
        let mut paths = BTreeMap::new();
        collect_object_paths(&stage, "", &mut paths);
        assert_eq!(
            paths.keys().map(String::as_str).collect::<Vec<_>>(),
            ["keep"]
        );
        assert_eq!(
            field(serialized_path(&stage, "/0").unwrap(), "unknown"),
            Some(&Value::Ext(4, vec![0, 255, 1]))
        );
        assert_eq!(
            field(serialized_path(&stage, "/1").unwrap(), "metadata"),
            Some(&Value::Binary(vec![3, 4]))
        );
    }
    #[test]
    fn layout_measurements_do_not_dirty_history_or_leak_into_move_undo() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        assert!(doc.nodes[0].auto_size);
        assert!(!doc.nodes[1].auto_size);
        assert!(doc.apply_layout_size(0, [200.0, 120.0]));
        assert!(doc.apply_layout_size(1, [400.0, 130.0]));
        assert_eq!(&doc.nodes[1].rect[2..], &[100.0, 130.0]);
        assert!(!doc.dirty());
        assert!(!doc.history.can_undo());
        let revision = doc.revision;
        assert!(!doc.apply_layout_size(0, [200.001, 120.001]));
        assert!(!doc.apply_layout_size(0, [f32::NAN, 20.0]));
        assert!(!doc.apply_layout_size(0, [20.0, -1.0]));
        assert!(!doc.apply_layout_size(99, [20.0, 20.0]));
        assert_eq!(doc.revision, revision);
        doc.begin("move");
        doc.nodes[0].rect[0] += 30.0;
        doc.nodes[1].rect[0] += 30.0;
        assert!(!doc.apply_layout_size(0, [300.0, 150.0]));
        doc.finish();
        assert!(doc.apply_layout_size(0, [220.0, 140.0]));
        assert!(doc.apply_layout_size(1, [900.0, 150.0]));
        doc.undo();
        assert_eq!(doc.nodes[0].rect, [0.0, 0.0, 220.0, 140.0]);
        assert_eq!(doc.nodes[1].rect, [0.0, 0.0, 100.0, 150.0]);
        assert!(!doc.dirty());
        doc.redo();
        assert_eq!(doc.nodes[0].rect, [30.0, 0.0, 220.0, 140.0]);
        assert_eq!(doc.nodes[1].rect, [30.0, 0.0, 100.0, 150.0]);
        doc.undo();
        doc.begin("resize manual node");
        doc.nodes[1].rect[2] = 200.0;
        doc.finish();
        doc.apply_layout_size(1, [200.0, 240.0]);
        doc.undo();
        assert_eq!(&doc.nodes[1].rect[2..], &[100.0, 150.0]);
    }
    #[test]
    fn text_edit_save_preserves_automatic_and_manual_layout_modes() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        assert!(doc.edit_text("node-a", "自动尺寸文字".into(), [300.0, 150.0]));
        assert!(doc.edit_text("node-b", "固定宽度文字".into(), [100.0, 200.0]));
        let path = dir.path().join("layout.prg");
        doc.save(&path).unwrap();
        let saved = ProjectFile::load(&path).unwrap();
        let objects = saved.objects();
        assert_eq!(objects["node-a"]["sizeAdjust"], "auto");
        assert_eq!(objects["node-b"]["sizeAdjust"], "manual");
        let loaded = Document::open(&path).unwrap();
        assert!(same_nodes(&loaded.nodes, &doc.nodes));
        doc.begin("change layout mode");
        doc.nodes[0].auto_size = false;
        doc.nodes[1].auto_size = true;
        doc.finish();
        doc.save(&path).unwrap();
        let saved = ProjectFile::load(&path).unwrap();
        let objects = saved.objects();
        assert_eq!(objects["node-a"]["sizeAdjust"], "manual");
        assert_eq!(objects["node-b"]["sizeAdjust"], "auto");
        assert!(same_nodes(
            &Document::open(&path).unwrap().nodes,
            &doc.nodes
        ));
    }
    #[test]
    fn added_edge_is_an_atomic_history_entry_and_rejects_invalid_input() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let original = doc.edges.clone();
        for id in ["", "node-a", "original-edge", "reserved-association"] {
            assert!(
                doc.add_edge(GraphEdge {
                    id: id.into(),
                    ..new_edge()
                })
                .is_err()
            );
        }
        assert!(
            doc.add_edge(GraphEdge {
                members: vec![0, 99],
                ..new_edge()
            })
            .is_err()
        );
        assert!(
            doc.add_edge(GraphEdge {
                rates: vec![[f32::NAN, 0.5]; 2],
                ..new_edge()
            })
            .is_err()
        );
        assert!(!doc.dirty());
        assert_eq!(doc.revision, 0);
        doc.begin("move");
        doc.nodes[0].rect[0] += 30.0;
        doc.add_edge(new_edge()).unwrap();
        assert_eq!(doc.edges.len(), original.len() + 1);
        assert!(doc.add_edge(new_edge()).is_err());
        doc.undo();
        assert_eq!(doc.edges, original);
        assert_eq!(doc.nodes[0].rect[0], 30.0);
        doc.undo();
        assert_eq!(doc.nodes[0].rect[0], 0.0);
        assert!(!doc.dirty());
        doc.redo();
        doc.redo();
        assert_eq!(doc.edges.last(), Some(&new_edge()));
        doc.undo();
        doc.add_edge(GraphEdge {
            id: "replacement-edge".into(),
            ..new_edge()
        })
        .unwrap();
        assert!(!doc.history.can_redo());
        assert_eq!(doc.edges.len(), original.len() + 1);
    }
    #[test]
    fn added_edge_save_preserves_references_unknown_data_and_undo_after_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = small_document(dir.path());
        let original = ProjectFile::load(doc.path.as_ref().unwrap()).unwrap().stage;
        let path = dir.path().join("added.prg");
        let mut edge = new_edge();
        edge.arrow = false;
        doc.add_edge(edge).unwrap();
        for _ in 0..2 {
            doc.save(&path).unwrap();
            let saved = ProjectFile::load(&path).unwrap();
            let items = saved.stage.as_array().unwrap();
            assert_eq!(&items[..4], original.as_array().unwrap());
            assert_eq!(items.len(), 5);
            assert_eq!(items[4]["associationList"][0]["$"], "/0/associationList/1");
            assert_eq!(items[4]["associationList"][1]["$"], "/0/associationList/0");
            assert_eq!(items[4]["arrowType"], "none");
            assert_eq!(items[4]["sourceRectangleRate"]["_"], "Vector");
            assert_eq!(items[4]["color"]["_"], "Color");
            let loaded = Graph::from_file(saved).unwrap();
            assert!(same_edges(&loaded, &doc.graph));
            let mut zip = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
            let mut unknown = vec![];
            zip.by_name("unknown.bin")
                .unwrap()
                .read_to_end(&mut unknown)
                .unwrap();
            assert_eq!(unknown, [0, 255, 4, 128]);
        }
        doc.undo();
        assert!(doc.dirty());
        doc.save(&path).unwrap();
        assert_eq!(ProjectFile::load(&path).unwrap().stage, original);
        doc.redo();
        doc.save(&path).unwrap();
        assert_eq!(Document::open(&path).unwrap().edges.len(), 2);
        let committed = fs::read(&path).unwrap();
        doc.edges[0].line_type = "unsupported direct mutation".into();
        assert!(doc.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), committed);
    }
    fn tutorial() -> Document {
        Document::open(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/tutorial-shortcut-keys-3.1-original.prg"),
        )
        .unwrap()
    }
    #[test]
    #[ignore = "requires pinned tutorial from run.sh"]
    fn tutorial_cut_edges_and_entire_canvas_save_reopen_and_undo_losslessly() {
        let mut doc = tutorial();
        let original_stage = ProjectFile::load(doc.path.as_ref().unwrap()).unwrap().stage;
        let original_nodes = doc.nodes.clone();
        let original_edges = doc.edges.clone();
        let original_archive = doc.archive.clone().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("cut-tutorial.prg");
        let edges: HashSet<_> = (0..doc.edges.len()).collect();
        let edge_plan = doc.deletion_plan(&HashSet::new(), &edges).unwrap();
        doc.delete_objects(&HashSet::new(), &edges).unwrap();
        assert_eq!(
            doc.nodes.len(),
            original_nodes.len() - edge_plan.nodes.len()
        );
        assert_eq!(
            doc.edges.len(),
            original_edges.len() - edge_plan.edges.len()
        );
        doc.save(&output).unwrap();
        let loaded = Document::open(&output).unwrap();
        assert!(same_nodes(&loaded.nodes, &doc.nodes));
        assert!(same_edges(&loaded, &doc));
        doc.undo();
        assert_eq!(doc.nodes, original_nodes);
        assert_eq!(doc.edges, original_edges);
        let nodes: HashSet<_> = (0..doc.nodes.len()).collect();
        let protected = doc.protected_nodes();
        doc.delete_objects(&nodes, &edges).unwrap();
        assert_eq!(doc.nodes.len(), protected.len());
        doc.save(&output).unwrap();
        let loaded = Document::open(&output).unwrap();
        assert!(same_nodes(&loaded.nodes, &doc.nodes));
        assert!(same_edges(&loaded, &doc));
        doc.undo();
        doc.save(&output).unwrap();
        assert_eq!(ProjectFile::load(&output).unwrap().stage, original_stage);
        let mut original = zip::ZipArchive::new(Cursor::new(original_archive)).unwrap();
        let mut saved = zip::ZipArchive::new(fs::File::open(output).unwrap()).unwrap();
        assert_eq!(original.len(), saved.len());
        for index in 0..original.len() {
            let mut entry = original.by_index(index).unwrap();
            if entry.name() == "stage.msgpack" {
                continue;
            }
            let mut before = vec![];
            let mut after = vec![];
            entry.read_to_end(&mut before).unwrap();
            saved
                .by_name(entry.name())
                .unwrap()
                .read_to_end(&mut after)
                .unwrap();
            assert_eq!(before, after);
        }
    }
    #[test]
    #[ignore = "requires pinned tutorial from run.sh"]
    fn tutorial_alignment_group_roundtrip_preserves_archive_edges_and_undo() {
        let mut doc = tutorial();
        let original_archive = doc.archive.clone().unwrap();
        doc.refresh_group_bounds().unwrap();
        assert!(!doc.dirty());
        let before = doc.nodes.clone();
        let selection: HashSet<_> = ["上对齐", "下对齐", "左对齐", "右对齐"]
            .into_iter()
            .map(|text| {
                doc.nodes
                    .iter()
                    .position(|node| node.kind == "TextNode" && node.text.trim() == text)
                    .unwrap_or_else(|| panic!("pinned tutorial must contain alignment node {text}"))
            })
            .collect();
        assert_eq!(selection.len(), 4);
        let selected_ids: HashSet<_> = selection.iter().map(|&i| doc.nodes[i].id.clone()).collect();
        let dir = tempfile::tempdir().unwrap();
        let baseline = dir.path().join("refreshed.prg");
        doc.save(&baseline).unwrap();
        let baseline_stage = ProjectFile::load(&baseline).unwrap().stage;
        let group = doc.create_group(&selection).unwrap();
        assert_eq!(doc.nodes.len(), before.len() + 1);
        assert_eq!(
            doc.nodes[group]
                .children
                .iter()
                .cloned()
                .collect::<HashSet<_>>(),
            selected_ids
        );
        let bounds = doc.nodes[group].rect;
        for &index in &selection {
            let member = doc.nodes[index].rect;
            assert!(bounds[0] <= member[0] && bounds[1] <= member[1]);
            assert!(bounds[0] + bounds[2] >= member[0] + member[2]);
            assert!(bounds[1] + bounds[3] >= member[1] + member[3]);
        }
        let grouped = dir.path().join("alignment-group.prg");
        doc.save(&grouped).unwrap();
        let loaded = Document::open(&grouped).unwrap();
        assert!(same_nodes(&loaded.nodes, &doc.nodes));
        assert!(same_edges(&loaded, &doc));
        doc.undo();
        assert!(same_nodes(&doc.nodes, &before));
        let restored = dir.path().join("undo-group.prg");
        doc.save(&restored).unwrap();
        let loaded = Document::open(&restored).unwrap();
        assert!(same_nodes(&loaded.nodes, &before));
        assert!(same_edges(&loaded, &doc));
        assert_eq!(ProjectFile::load(&restored).unwrap().stage, baseline_stage);
        for path in [&grouped, &restored] {
            let mut original = zip::ZipArchive::new(Cursor::new(&original_archive)).unwrap();
            let mut saved = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
            assert_eq!(original.len(), saved.len());
            for index in 0..original.len() {
                let mut entry = original.by_index(index).unwrap();
                if entry.name() == "stage.msgpack" {
                    continue;
                }
                let mut original_bytes = vec![];
                let mut saved_bytes = vec![];
                entry.read_to_end(&mut original_bytes).unwrap();
                saved
                    .by_name(entry.name())
                    .unwrap()
                    .read_to_end(&mut saved_bytes)
                    .unwrap();
                assert_eq!(
                    original_bytes,
                    saved_bytes,
                    "archive entry {} changed",
                    entry.name()
                );
            }
        }
    }
    #[test]
    #[ignore = "requires pinned tutorial from run.sh"]
    fn edit_move_save_roundtrip_preserves_archive_and_history() {
        let mut doc = tutorial();
        let index = doc.nodes.iter().position(|n| n.kind == "TextNode").unwrap();
        let before = doc.nodes.clone();
        let id = doc.nodes[index].id.clone();
        let size = [doc.nodes[index].rect[2], 200.0];
        assert!(doc.edit_text(&id, "中文\nMarkdown **文字** 🙂".into(), size));
        doc.begin("批量移动");
        for node in &mut doc.nodes {
            node.rect[0] += 24.0;
            node.rect[1] -= 12.0;
        }
        doc.finish();
        let after = doc.nodes.clone();
        doc.undo();
        doc.undo();
        assert_eq!(doc.nodes, before);
        assert!(!doc.dirty());
        doc.redo();
        doc.redo();
        assert_eq!(doc.nodes, after);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.prg");
        doc.save(&path).unwrap();
        assert!(!doc.dirty());
        let loaded = Document::open(&path).unwrap();
        assert!(same_nodes(&loaded.nodes, &after));
        let mut a = zip::ZipArchive::new(Cursor::new(doc.archive.as_ref().unwrap())).unwrap();
        let mut b = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(a.len(), b.len());
        for i in 0..a.len() {
            let mut entry = a.by_index(i).unwrap();
            if entry.name() == "stage.msgpack" {
                continue;
            }
            let mut x = vec![];
            let mut y = vec![];
            entry.read_to_end(&mut x).unwrap();
            b.by_name(entry.name())
                .unwrap()
                .read_to_end(&mut y)
                .unwrap();
            assert_eq!(x, y);
        }
        fs::write(&path, b"external modification").unwrap();
        doc.begin("unsaved move");
        doc.nodes[index].rect[0] += 10.0;
        doc.finish();
        assert!(doc.save(&path).is_err());
        let alias = dir.path().join(".").join("roundtrip.prg");
        assert!(doc.save(&alias).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external modification");
        assert!(doc.dirty());
        assert!(
            doc.save(&dir.path().join("missing-parent/fail.prg"))
                .is_err()
        );
        assert!(doc.dirty());
    }
    #[test]
    fn recovery_snapshots_preserve_save_point_source_and_conflict_detection() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.prg");
        let snapshot = dir.path().join("recovery.prg");
        let mut doc = Document::blank().unwrap();
        doc.save(&source).unwrap();
        let original = fs::read(&source).unwrap();
        doc.create_text_node([20.0, 40.0]).unwrap();
        let path_before = doc.path.clone();
        doc.save_snapshot(&snapshot).unwrap();
        assert!(doc.dirty());
        assert_eq!(doc.path, path_before);
        assert_eq!(fs::read(&source).unwrap(), original);
        assert!(same_nodes(
            &Document::open(&snapshot).unwrap().nodes,
            &doc.nodes
        ));
        assert!(doc.save_snapshot(&source).is_err());
        assert!(doc.save_snapshot(&snapshot).is_err());
        assert_eq!(fs::read(&source).unwrap(), original);
        doc.undo();
        assert!(!doc.dirty());
        doc.redo();
        assert!(doc.dirty());
        fs::write(&source, b"external edit").unwrap();
        doc.save_snapshot(&dir.path().join("second.prg")).unwrap();
        assert!(doc.save(&source).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"external edit");
        assert!(doc.dirty());
    }

    #[test]
    fn recovery_snapshot_of_draft_rejects_pending_edits_and_failed_writes() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let path = dir.path().join("draft.prg");
        doc.begin("moving");
        doc.nodes[0].rect[0] += 10.0;
        assert!(doc.save_snapshot(&path).is_err());
        assert!(!path.exists());
        doc.finish();
        assert!(
            doc.save_snapshot(&dir.path().join("missing/draft.prg"))
                .is_err()
        );
        doc.save_snapshot(&path).unwrap();
        assert!(doc.path.is_none());
        assert!(doc.dirty());
        assert!(same_nodes(
            &Document::open(&path).unwrap().nodes,
            &doc.nodes
        ));
    }

    #[test]
    fn project_bytes_decode_one_snapshot_after_the_disk_file_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snapshot.prg");
        let mut document = Document::blank().unwrap();
        document.create_text_node([10.0, 20.0]).unwrap();
        document
            .attachments
            .insert("source.txt".into(), b"original".to_vec());
        document.save(&path).unwrap();
        let bytes = fs::read(&path).unwrap();

        document.create_text_node([30.0, 40.0]).unwrap();
        document
            .attachments
            .insert("source.txt".into(), b"changed".to_vec());
        document.save(&path).unwrap();

        let snapshot = Graph::from_file(ProjectFile::from_bytes(&bytes).unwrap()).unwrap();
        assert_eq!(snapshot.nodes.len(), 1);
        assert_eq!(snapshot.attachments["source.txt"], b"original");
        let current = Document::open(&path).unwrap();
        assert_eq!(current.nodes.len(), 2);
        assert_eq!(current.attachments["source.txt"], b"changed");
        assert_eq!(current.archive.as_ref(), current.disk_bytes.as_ref());
        assert!(ProjectFile::from_bytes(&bytes[..bytes.len() / 2]).is_err());
    }

    #[test]
    fn mio_conversion_roundtrips_identity_assets_and_unknown_data() {
        let directory = tempfile::tempdir().unwrap();
        let mut document = small_document(directory.path());
        let legacy = document.path.clone().unwrap();
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&legacy)
            .unwrap();
        let mut extra = zip::ZipWriter::new_append(file).unwrap();
        extra
            .start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        extra
            .write_all(br#"{"format":"third-party-extension"}"#)
            .unwrap();
        extra
            .start_file("canvas/notes.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        extra.write_all(b"opaque legacy data").unwrap();
        extra.finish().unwrap();
        document = Document::open(&legacy).unwrap();
        let original = fs::read(document.path.as_ref().unwrap()).unwrap();
        let id = document.project_id().to_owned();
        document
            .attachments
            .insert("image.png".into(), vec![0, 255, 1]);
        document
            .manifest
            .extensions
            .insert("producerNote".into(), serde_json::json!({"keep": true}));
        let path = directory.path().join("新项目.mio");
        document.save(&path).unwrap();
        assert_eq!(
            fs::read(directory.path().join("original.prg")).unwrap(),
            original
        );

        let mut zip = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        assert!(zip.by_name("stage.msgpack").is_err());
        assert!(zip.by_name("canvas/stage.msgpack").is_ok());
        assert_eq!(
            project::read_entry(&mut zip, "extensions/legacy/manifest.json", 1024).unwrap(),
            br#"{"format":"third-party-extension"}"#
        );
        assert_eq!(
            project::read_entry(&mut zip, "extensions/legacy/canvas/notes.bin", 1024).unwrap(),
            b"opaque legacy data"
        );
        assert_eq!(
            project::read_entry(&mut zip, "extensions/legacy/unknown.bin", 1024).unwrap(),
            [0, 255, 4, 128]
        );
        assert_eq!(
            project::read_entry(&mut zip, "assets/image.png", 1024).unwrap(),
            [0, 255, 1]
        );
        drop(zip);

        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut zip = zip::ZipWriter::new_append(file).unwrap();
        zip.start_file(
            "extensions/future/data.bin",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&[7, 0, 128]).unwrap();
        zip.finish().unwrap();

        let mut reopened = Document::open(&path).unwrap();
        assert_eq!(reopened.project_id(), id);
        assert_eq!(reopened.manifest, document.manifest);
        assert_eq!(reopened.attachments, document.attachments);
        assert!(same_nodes(&reopened.nodes, &document.nodes));
        let node_id = reopened.nodes[0].id.clone();
        reopened.edit_text(&node_id, "更新后的画布".into(), [100.0, 80.0]);
        let copy = directory.path().join("另存副本.mio");
        reopened.save(&copy).unwrap();
        let again = Document::open(&copy).unwrap();
        assert_eq!(again.project_id(), id);
        assert_eq!(again.nodes[0].text, "更新后的画布");
        assert_eq!(again.manifest, document.manifest);
        let mut zip = zip::ZipArchive::new(fs::File::open(copy).unwrap()).unwrap();
        assert_eq!(
            project::read_entry(&mut zip, "extensions/future/data.bin", 1024).unwrap(),
            [7, 0, 128]
        );
        assert_eq!(
            project::read_entry(&mut zip, "extensions/legacy/unknown.bin", 1024).unwrap(),
            [0, 255, 4, 128]
        );
        let parsed = ProjectFile::load(&path).unwrap();
        assert_eq!(
            parsed.objects()["original-edge"]["unknown"],
            serde_json::json!({"keep": [1, 2, 3]})
        );
        assert!(reopened.save(&directory.path().join("lossy.prg")).is_err());
    }

    #[test]
    fn mio_history_is_portable_and_updates_do_not_remove_other_workflows() {
        use crate::project_history::ArchivedWorkflow;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("协作.mio");
        let mut document = Document::blank().unwrap();
        let workflow = |id: &str, goal: &str| ArchivedWorkflow {
            id: id.into(),
            goal: goal.into(),
            model: "test/model".into(),
            files: vec![],
            plan: None,
            draft_plan: None,
            runs: vec![],
        };
        let history = |workflows| History {
            schema_version: 1,
            workflows,
        };
        document
            .attach_history(history(vec![
                workflow("one", "original"),
                workflow("two", "keep"),
            ]))
            .unwrap();
        assert!(document.dirty());
        document.save(&path).unwrap();
        assert!(!document.dirty());
        document
            .attach_history(history(vec![workflow("one", "updated")]))
            .unwrap();
        // Canvas undo does not rewrite collaboration evidence or clear its dirty flag.
        document.undo();
        assert!(document.dirty());
        assert_eq!(document.history_archive().workflows.len(), 2);
        let recovery = directory.path().join("recovery.mio");
        document
            .capture_snapshot()
            .unwrap()
            .save_snapshot(&recovery)
            .unwrap();
        let recovered = Document::open_recovery(&recovery).unwrap();
        assert_eq!(recovered.project_id(), document.project_id());
        assert_eq!(recovered.history_archive(), document.history_archive());
        assert!(recovered.dirty());
        assert!(recovered.path.is_none());
        let old_path = directory.path().join("do-not-overwrite.prg");
        fs::write(&old_path, b"existing file").unwrap();
        assert!(document.save(&old_path).is_err());
        assert_eq!(fs::read(old_path).unwrap(), b"existing file");
        document.save(&path).unwrap();
        assert_eq!(
            Document::open(&path).unwrap().history_archive(),
            document.history_archive()
        );
        fs::write(&path, b"external change").unwrap();
        assert!(document.save(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), b"external change");
    }

    #[test]
    fn mio_future_or_incomplete_files_cannot_replace_a_session() {
        let directory = tempfile::tempdir().unwrap();
        let mut session = crate::session::Session::new().unwrap();
        session
            .execute(crate::session::Command::AddText { x: 0.0, y: 0.0 })
            .unwrap();
        let original_id = session.project_id().to_owned();
        for (schema, history, unsafe_entry) in
            [(2, true, false), (1, false, false), (1, true, true)]
        {
            let path = directory.path().join("invalid.mio");
            let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            let manifest = Manifest {
                schema_version: schema,
                ..Default::default()
            };
            zip.start_file(
                project::MANIFEST_ENTRY,
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            serde_json::to_writer(&mut zip, &manifest).unwrap();
            zip.start_file(
                Format::Mio.stage(),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(&[0x90]).unwrap();
            if history {
                zip.start_file(
                    project::HISTORY_ENTRY,
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
                serde_json::to_writer(&mut zip, &History::default()).unwrap();
            }
            if unsafe_entry {
                zip.start_file(
                    "assets/../outside",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            }
            zip.finish().unwrap();
            let before = fs::read(&path).unwrap();
            assert!(
                session
                    .execute(crate::session::Command::Open {
                        path: path.to_string_lossy().into(),
                        discard: true
                    })
                    .is_err()
            );
            assert_eq!(session.project_id(), original_id);
            assert_eq!(session.snapshot().nodes.len(), 1);
            assert!(session.snapshot().dirty);
            assert_eq!(fs::read(path).unwrap(), before);
        }
    }

    #[test]
    fn new_project_without_an_extension_is_saved_as_mio() {
        let directory = tempfile::tempdir().unwrap();
        let mut document = Document::blank().unwrap();
        let id = document.project_id().to_owned();
        document.save(&directory.path().join("未命名项目")).unwrap();
        let path = directory.path().join("未命名项目.mio");
        assert_eq!(document.path.as_deref(), Some(path.as_path()));
        assert_eq!(Document::open(&path).unwrap().project_id(), id);
        assert!(!document.dirty());
    }

    #[test]
    fn mio_archive_updates_reject_stale_or_conflicting_fixed_runs() {
        let directory = tempfile::tempdir().unwrap();
        let mut document = Document::blank().unwrap();
        let run = serde_json::json!({
            "id":"first", "taskId":"$plan", "status":"succeeded", "createdAt":1,
            "finishedAt":2, "sources":[], "prompt":"fixed input", "inputFiles":[],
            "output":"fixed result", "proposal":null, "error":null, "review":null,
            "usage":null, "retryOf":null
        });
        let mut second = run.clone();
        second["id"] = "second".into();
        let history: History = serde_json::from_value(serde_json::json!({
            "schemaVersion":1,
            "workflows":[{"id":"workflow", "goal":"preserve evidence", "model":"test/model",
                "files":[], "plan":null, "draftPlan":null, "runs":[run,second]}]
        }))
        .unwrap();
        document.attach_history(history.clone()).unwrap();
        document
            .save(&directory.path().join("evidence.mio"))
            .unwrap();
        let mut stale = history.clone();
        stale.workflows[0].runs.pop();
        assert!(document.attach_history(stale).is_err());
        let mut conflicting = history.clone();
        conflicting.workflows[0].runs[0].output = "different result".into();
        assert!(document.attach_history(conflicting).is_err());
        assert_eq!(document.history_archive(), &history);
        assert!(!document.dirty());
        let mut reviewed = history;
        reviewed.workflows[0].runs[0].review = Some("approved".into());
        document.attach_history(reviewed.clone()).unwrap();
        assert_eq!(document.history_archive(), &reviewed);
        assert!(document.dirty());
    }

    #[test]
    fn msgpack_unknown_values_survive_patch() {
        let mut stage = Value::Map(vec![
            ("unknown".into(), Value::Ext(4, vec![0, 255, 1])),
            ("binary".into(), Value::Binary(vec![3, 4])),
        ]);
        let before = stage.clone();
        patch_stage(&mut stage, &BTreeMap::new(), &BTreeMap::new()).unwrap();
        assert_eq!(stage, before);
    }
    #[test]
    fn nested_group_selection_deduplicates_cycles_and_respects_locks() {
        let node = |id: &str, children: Vec<String>, locked| GraphNode {
            id: id.into(),
            kind: if children.is_empty() {
                "TextNode"
            } else {
                "Section"
            }
            .into(),
            rect: [0.0, 0.0, 100.0, 100.0],
            text: String::new(),
            url: None,
            font_size: 32.0,
            auto_size: false,
            color: [0.0; 4],
            attachment: None,
            source_name: None,
            children,
            locked,
            collapsed: false,
            stroke: vec![],
        };
        let doc = Document::memory(Graph {
            nodes: vec![
                node("outer", vec!["inner".into(), "child".into()], false),
                node(
                    "inner",
                    vec!["outer".into(), "child".into(), "locked".into()],
                    false,
                ),
                node("child", vec![], false),
                node("locked", vec![], true),
            ],
            edges: vec![],
            attachments: Default::default(),
            counts: Default::default(),
        });
        assert_eq!(
            doc.expand_selection(&HashSet::from([0, 1, 3])),
            HashSet::from([0, 1, 2])
        );
    }
    #[test]
    #[ignore = "requires pinned tutorial from run.sh"]
    fn cancelled_and_empty_transactions_do_not_dirty_document() {
        let mut doc = tutorial();
        let before = doc.nodes.clone();
        doc.begin("noop");
        assert!(!doc.finish());
        assert!(!doc.dirty());
        doc.begin("cancel");
        doc.nodes[0].rect[0] += 100.0;
        doc.cancel();
        assert_eq!(doc.nodes, before);
        assert!(!doc.dirty());
        doc.begin("move");
        doc.nodes[0].rect[0] += 20.0;
        doc.finish();
        doc.undo();
        doc.begin("new move");
        doc.nodes[1].rect[0] += 30.0;
        doc.finish();
        assert!(!doc.history.can_redo());
    }
}

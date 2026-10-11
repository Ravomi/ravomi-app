use super::*;
use bevy_egui::egui;
use project_graph_bevy_canvas::document::NodeProperties;

#[derive(Clone, Copy)]
pub enum Command {
    NewNode,
    Edit,
    Copy,
    Cut,
    Paste,
    Delete,
    Reverse,
    ToggleLock,
    OpenUrl,
    Properties,
    Search,
}

pub enum Action {
    Undo,
    Group,
    Command(Command, Vec2),
    Focus(usize),
    Apply(HashSet<String>, NodeProperties),
}

struct Properties {
    ids: HashSet<String>,
    color: [u8; 4],
    font_size: f32,
    auto_size: bool,
    width: f32,
    changes: NodeProperties,
}

#[derive(Default)]
pub struct Tools {
    pub icons: Vec<egui::TextureHandle>,
    pub context: Option<(egui::Pos2, Vec2)>,
    properties: Option<Properties>,
    search: bool,
    query: String,
    focus_search: bool,
    search_composing: bool,
}

fn menu_button<'a>(
    icons: &'a [egui::TextureHandle],
    index: usize,
    label: &'a str,
    tint: egui::Color32,
) -> egui::Button<'a> {
    if let Some(icon) = icons.get(index) {
        egui::Button::new((
            egui::Image::new(icon)
                .fit_to_exact_size(egui::vec2(18.0, 18.0))
                .tint(tint),
            label,
        ))
        .frame(false)
    } else {
        let fallback = if label.is_empty() {
            match index {
                1 => "撤销",
                13 => "分组",
                27 => "复制",
                28 => "粘贴",
                29 => "删除",
                32 => "剪切",
                _ => "操作",
            }
        } else {
            label
        };
        egui::Button::new(fallback).frame(false)
    }
}

impl Tools {
    pub fn reset_for_document(&mut self) {
        let icons = std::mem::take(&mut self.icons);
        *self = Self { icons, ..default() };
    }

    pub fn preview(
        &mut self,
        mode: &str,
        doc: &Document,
        interaction: &mut Interaction,
    ) -> Result<()> {
        match mode {
            "search" => self.open_search(),
            "properties" | "context" => {
                interaction.selected = (0..doc.nodes.len()).take(4).collect();
                if mode == "properties" {
                    self.open_properties(doc, &interaction.selected);
                } else {
                    self.context = Some((egui::pos2(180.0, 120.0), Vec2::ZERO));
                }
            }
            _ => return Err("--preview-tool expects search, properties or context".into()),
        }
        Ok(())
    }

    pub fn visible(&self) -> bool {
        self.context.is_some() || self.properties.is_some() || self.search
    }

    pub fn open_search(&mut self) {
        self.properties = None;
        self.context = None;
        self.search = true;
        self.focus_search = true;
    }

    fn open_properties(&mut self, doc: &Document, selection: &HashSet<usize>) {
        self.search = false;
        self.context = None;
        let Some(node) = selection.iter().min().and_then(|&i| doc.nodes.get(i)) else {
            return;
        };
        self.properties = Some(Properties {
            ids: selection
                .iter()
                .filter_map(|&i| doc.nodes.get(i))
                .map(|n| n.id.clone())
                .collect(),
            color: node.color.map(|v| (v * 255.0).round() as u8),
            font_size: node.font_size,
            auto_size: node.auto_size,
            width: node.rect[2],
            changes: default(),
        });
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        light: bool,
        doc: &Document,
        interaction: &Interaction,
        clipboard_empty: bool,
    ) -> Option<Action> {
        let mut action = None;
        if let Some((screen, point)) = self.context {
            let has_nodes = !interaction.selected.is_empty();
            let has_edges = !interaction.selected_edges.is_empty();
            let area = egui::Area::new("canvas-context".into())
                .order(egui::Order::Foreground)
                .fixed_pos(screen)
                .show(ctx, |ui| {
                    theme::tool_frame(light).inner_margin(6).show(ui, |ui| {
                        theme::tool_style(ui, light);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if has_nodes {
                            let p = theme::chrome_palette(light);
                            ui.set_width(264.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().button_padding.x = 4.0;
                                ui.spacing_mut().item_spacing.x = 8.0;
                                for (icon, hint, enabled, next) in [
                                    (
                                        27,
                                        "复制 · Ctrl+C",
                                        true,
                                        Action::Command(Command::Copy, point),
                                    ),
                                    (
                                        28,
                                        "粘贴 · Ctrl+V",
                                        !clipboard_empty,
                                        Action::Command(Command::Paste, point),
                                    ),
                                    (
                                        32,
                                        "剪切 · Ctrl+X",
                                        true,
                                        Action::Command(Command::Cut, point),
                                    ),
                                    (
                                        29,
                                        "删除 · Delete",
                                        true,
                                        Action::Command(Command::Delete, point),
                                    ),
                                    (1, "撤销 · Ctrl+Z", doc.history.can_undo(), Action::Undo),
                                    (13, "创建分组 · Ctrl+G", true, Action::Group),
                                ] {
                                    if ui
                                        .add_enabled(
                                            enabled,
                                            menu_button(&self.icons, icon, "", p.foreground)
                                                .min_size(egui::vec2(36.0, 36.0)),
                                        )
                                        .on_hover_text(hint)
                                        .clicked()
                                    {
                                        action = Some(next);
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                for label in ["对齐与分布", "自动布局"] {
                                    ui.add_enabled(
                                        false,
                                        egui::Button::new(label)
                                            .frame(false)
                                            .min_size(egui::vec2(128.0, 32.0)),
                                    )
                                    .on_disabled_hover_text("此功能尚未接入 Bevy 原型");
                                }
                            });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                let swatches = std::iter::once((
                                    "无颜色／跟随主题",
                                    egui::Color32::TRANSPARENT,
                                ))
                                .chain(theme::node_swatches(light));
                                for (label, color) in swatches {
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(24.0, 24.0),
                                        egui::Sense::click(),
                                    );
                                    if color == egui::Color32::TRANSPARENT {
                                        ui.painter().rect_filled(rect, 0, p.surface);
                                        ui.painter().line_segment(
                                            [
                                                rect.left_bottom() + egui::vec2(5.0, -5.0),
                                                rect.right_top() + egui::vec2(-5.0, 5.0),
                                            ],
                                            egui::Stroke::new(1.0, p.muted),
                                        );
                                    } else {
                                        ui.painter().rect_filled(rect, 0, color);
                                    }
                                    if response.hovered() || response.has_focus() {
                                        ui.painter().rect_stroke(
                                            rect.shrink(1.0),
                                            0,
                                            egui::Stroke::new(2.0, p.foreground),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    if response.on_hover_text(label).clicked() {
                                        let ids = interaction
                                            .selected
                                            .iter()
                                            .filter_map(|&i| doc.nodes.get(i))
                                            .map(|n| n.id.clone())
                                            .collect();
                                        action = Some(Action::Apply(
                                            ids,
                                            NodeProperties {
                                                color: Some(
                                                    color.to_array().map(|v| v as f32 / 255.0),
                                                ),
                                                ..default()
                                            },
                                        ));
                                    }
                                }
                            });
                            ui.add_space(6.0);
                            let editable = interaction.selected.len() == 1
                                && interaction
                                    .selected
                                    .iter()
                                    .next()
                                    .is_some_and(|&i| doc.can_edit_text(i));
                            let can_open = interaction.selected.iter().all(|&i| {
                                doc.nodes
                                    .get(i)
                                    .is_some_and(|node| url_open::target(node).is_ok())
                            });
                            if ui
                                .add_enabled(
                                    can_open,
                                    menu_button(&self.icons, 26, "打开网址", p.foreground)
                                        .shortcut_text("Ctrl+E")
                                        .min_size(egui::vec2(264.0, 36.0)),
                                )
                                .clicked()
                            {
                                action = Some(Action::Command(Command::OpenUrl, point));
                            }
                            let all_locked =
                                interaction.selected.iter().all(|&i| doc.nodes[i].locked);
                            if ui
                                .add(
                                    egui::Button::new(if all_locked {
                                        "解锁节点"
                                    } else {
                                        "锁定节点"
                                    })
                                    .shortcut_text("Ctrl+L")
                                    .frame(false)
                                    .min_size(egui::vec2(264.0, 36.0)),
                                )
                                .clicked()
                            {
                                action = Some(Action::Command(Command::ToggleLock, point));
                            }
                            for (icon, label, shortcut, enabled, command) in [
                                (31, "更改颜色", "Alt+Enter", true, Command::Properties),
                                (30, "编辑文字／分组名称", "F2", editable, Command::Edit),
                            ] {
                                if ui
                                    .add_enabled(
                                        enabled,
                                        menu_button(&self.icons, icon, label, p.foreground)
                                            .shortcut_text(shortcut)
                                            .min_size(egui::vec2(264.0, 36.0)),
                                    )
                                    .clicked()
                                {
                                    action = Some(Action::Command(command, point));
                                }
                            }
                            return;
                        }
                        let mut row = |label, shortcut, enabled, command| {
                            if ui
                                .add_enabled(
                                    enabled,
                                    egui::Button::new(label)
                                        .shortcut_text(shortcut)
                                        .frame(false)
                                        .min_size(egui::vec2(216.0, 32.0)),
                                )
                                .clicked()
                            {
                                action = Some(Action::Command(command, point));
                            }
                        };
                        if has_nodes || has_edges {
                            let editable = (interaction.selected.len() == 1
                                && interaction
                                    .selected
                                    .iter()
                                    .next()
                                    .is_some_and(|&i| doc.can_edit_text(i)))
                                || (interaction.selected.is_empty()
                                    && interaction.selected_edges.len() == 1);
                            row("编辑", "F2", editable, Command::Edit);
                            if has_nodes {
                                row("复制", "Ctrl+C", true, Command::Copy);
                                row("剪切", "Ctrl+X", true, Command::Cut);
                                row("节点属性", "Alt+Enter", true, Command::Properties);
                            } else {
                                row(
                                    "反转方向",
                                    "Ctrl+R",
                                    interaction.selected_edges.len() == 1
                                        && doc.edges.iter().any(|e| {
                                            interaction.selected_edges.contains(&e.id)
                                                && e.kind == "LineEdge"
                                        }),
                                    Command::Reverse,
                                );
                            }
                            row("删除", "Delete", true, Command::Delete);
                        } else {
                            row("新建节点", "双击", true, Command::NewNode);
                            row("粘贴", "Ctrl+V", !clipboard_empty, Command::Paste);
                            row("搜索当前图", "Ctrl+F", true, Command::Search);
                        }
                    });
                });
            let outside = ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| !area.response.rect.contains(p))
            });
            if action.is_some() || outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.context = None;
            }
        }
        if self.search {
            let mut open = true;
            let mut ime_frame = self.search_composing;
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Ime(event) = event {
                        ime_frame = true;
                        match event {
                            egui::ImeEvent::Preedit { text, .. } => {
                                self.search_composing = !text.is_empty()
                            }
                            egui::ImeEvent::Commit(_) => self.search_composing = false,
                            _ => {}
                        }
                    }
                }
            });
            egui::Window::new("搜索当前图")
                .title_bar(false)
                .resizable(false)
                .frame(theme::tool_frame(light))
                .default_width(320.0)
                .default_pos(egui::pos2(24.0, 92.0))
                .show(ctx, |ui| {
                    theme::tool_style(ui, light);
                    open = !theme::tool_header(ui, "搜索当前图", light);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id(egui::Id::new("canvas-search-input"))
                            .hint_text("输入节点文字…")
                            .margin(egui::vec2(10.0, 8.0))
                            .desired_width(f32::INFINITY),
                    );
                    if self.focus_search {
                        response.request_focus();
                        self.focus_search = false;
                    }
                    let matches = doc.search_nodes(&self.query);
                    if !ime_frame
                        && (response.has_focus() || response.lost_focus())
                        && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                        && let Some(&index) = matches.first()
                    {
                        action = Some(Action::Focus(index));
                    }
                    if self.query.trim().is_empty() {
                        ui.weak("搜索当前标签页中的节点文字");
                    } else {
                        ui.weak(format!("找到 {} 个节点", matches.len()));
                    }
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for index in matches.iter().take(200) {
                                let node = &doc.nodes[*index];
                                if ui
                                    .add(
                                        egui::Button::new(
                                            node.text.lines().next().unwrap_or("空文字节点"),
                                        )
                                        .frame(false)
                                        .min_size(egui::vec2(ui.available_width(), 32.0))
                                        .truncate()
                                        .selected(interaction.selected.contains(index)),
                                    )
                                    .on_hover_text(&node.text)
                                    .clicked()
                                {
                                    action = Some(Action::Focus(*index));
                                }
                            }
                            if matches.len() > 200 {
                                ui.weak("仅显示前 200 项，请细化搜索文字。");
                            }
                        });
                });
            if !open || matches!(action, Some(Action::Focus(_))) {
                self.search = false;
            }
            if !ime_frame && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.search = false;
            }
        }
        if let Some(draft) = &mut self.properties {
            let mut open = true;
            let mut apply = false;
            egui::Window::new("节点属性")
                .title_bar(false)
                .frame(theme::tool_frame(light))
                .resizable(false)
                .default_width(300.0)
                .default_pos(egui::pos2(24.0, 92.0))
                .show(ctx, |ui| {
                    theme::tool_style(ui, light);
                    open = !theme::tool_header(ui, "节点属性", light);
                    ui.weak(format!(
                        "已选 {} 个节点 · 锁定节点不受影响",
                        draft.ids.len()
                    ));
                    ui.add_space(8.0);
                    egui::Grid::new("node-properties-fields")
                        .num_columns(2)
                        .spacing(egui::vec2(20.0, 12.0))
                        .show(ui, |ui| {
                            ui.label("颜色");
                            ui.horizontal(|ui| {
                                if ui
                                    .color_edit_button_srgba_unmultiplied(&mut draft.color)
                                    .changed()
                                {
                                    draft.changes.color =
                                        Some(draft.color.map(|v| v as f32 / 255.0));
                                }
                                if ui.button("跟随主题").clicked() {
                                    draft.color = [0; 4];
                                    draft.changes.color = Some([0.0; 4]);
                                }
                            });
                            ui.end_row();
                            ui.label("字号");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut draft.font_size)
                                        .range(2.0..=512.0)
                                        .speed(1.0),
                                )
                                .changed()
                            {
                                draft.changes.font_size = Some(draft.font_size);
                            }
                            ui.end_row();
                            ui.label("尺寸");
                            let auto_size = ui
                                .scope(|ui| {
                                    let p = theme::chrome_palette(light);
                                    let widgets = &mut ui.visuals_mut().widgets;
                                    for widget in [
                                        &mut widgets.inactive,
                                        &mut widgets.hovered,
                                        &mut widgets.active,
                                    ] {
                                        widget.corner_radius = egui::CornerRadius::same(4);
                                        widget.bg_stroke = egui::Stroke::new(1.0, p.muted);
                                    }
                                    ui.checkbox(&mut draft.auto_size, "自动宽度")
                                })
                                .inner;
                            if auto_size.changed() {
                                draft.changes.auto_size = Some(draft.auto_size);
                                draft.changes.width = (!draft.auto_size).then_some(draft.width);
                            }
                            ui.end_row();
                            ui.label("宽度");
                            if ui
                                .add_enabled(
                                    !draft.auto_size,
                                    egui::DragValue::new(&mut draft.width)
                                        .range(16.0..=100_000.0)
                                        .speed(1.0),
                                )
                                .changed()
                            {
                                draft.changes.width = Some(draft.width);
                            }
                            ui.end_row();
                        });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            apply = ui.add(theme::primary_button("应用", light)).clicked();
                            if ui.button("取消").clicked() {
                                open = false;
                            }
                        });
                    });
                });
            if apply {
                action = Some(Action::Apply(draft.ids.clone(), draft.changes.clone()));
            }
            if !open || apply {
                self.properties = None;
            }
        }
        action
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute(
    command: Command,
    point: Vec2,
    doc: &mut Document,
    interaction: &mut Interaction,
    editor: &mut editor::Editor,
    physics: &mut physics::PhysicsEditor,
    effects: &mut effects::Effects,
) -> Result<()> {
    doc.selection = project_graph_bevy_canvas::document::Selection::capture(
        doc,
        &interaction.selected,
        &interaction.selected_edges,
    );
    physics.finish(doc);
    interaction.creation.cancel();
    interaction.cutting.cancel();
    interaction.jump.stop(doc);
    match command {
        Command::NewNode => creation::create(point, doc, interaction, editor, physics, effects),
        Command::Copy => {
            editor.clipboard.copy(doc, &interaction.selected)?;
            editor.clipboard_copy_text = Some("Ravomi 节点（在 Ravomi 中粘贴）".into());
        }
        Command::Cut => {
            if editor.clipboard.cut(doc, &interaction.selected)? {
                editor.clipboard_copy_text = Some("Ravomi 节点（在 Ravomi 中粘贴）".into());
                interaction.selected.clear();
                interaction.selected_edges.clear();
            }
        }
        Command::Paste => {
            let selected = editor.clipboard.paste(doc, [point.x, -point.y])?;
            if !selected.is_empty() {
                interaction.selected = selected;
                interaction.selected_edges.clear();
            }
        }
        Command::Delete => {
            let ids: HashSet<_> = interaction
                .selected
                .iter()
                .filter_map(|&i| doc.nodes.get(i))
                .map(|n| n.id.clone())
                .collect();
            let edges = doc
                .edges
                .iter()
                .enumerate()
                .filter(|(_, e)| interaction.selected_edges.contains(&e.id))
                .map(|(i, _)| i)
                .collect();
            doc.delete_objects(&interaction.selected, &edges)?;
            interaction.selected = doc
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| ids.contains(&n.id))
                .map(|(i, _)| i)
                .collect();
            interaction
                .selected_edges
                .retain(|id| doc.edges.iter().any(|e| &e.id == id));
        }
        Command::Edit => {
            let protected = doc.protected_nodes();
            if interaction.selected.len() == 1 {
                if let Some(&i) = interaction.selected.iter().next()
                    && doc.can_edit_text(i)
                {
                    editor.begin(&doc.nodes[i]);
                } else {
                    return Err("请选择未锁定的文字节点或分组".into());
                }
            } else if interaction.selected_edges.len() == 1
                && let Some(edge) = doc
                    .edges
                    .iter()
                    .find(|e| interaction.selected_edges.contains(&e.id))
            {
                if edge.members.iter().any(|i| protected.contains(i)) {
                    return Err("锁定节点的连线不能修改".into());
                }
                editor.begin_edge(edge);
            }
        }
        Command::Reverse => {
            if interaction.selected_edges.len() == 1
                && let Some(index) = doc
                    .edges
                    .iter()
                    .position(|e| interaction.selected_edges.contains(&e.id))
            {
                let title = doc.edges[index].text.clone();
                doc.edit_edge(index, title, true)?;
            }
        }
        Command::ToggleLock => {
            let locked = interaction
                .selected
                .iter()
                .any(|&i| doc.nodes.get(i).is_some_and(|node| !node.locked));
            doc.set_nodes_locked(&interaction.selected, locked)?;
        }
        Command::OpenUrl => editor.open_urls(doc, &interaction.selected)?,
        Command::Properties => editor.tools.open_properties(doc, &interaction.selected),
        Command::Search => editor.tools.open_search(),
    }
    interaction.selection_revision = doc.revision;
    interaction.moving.clear();
    interaction.drag_start = None;
    interaction.linking = None;
    interaction.dirty = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_lock_command_locks_a_mixed_selection_then_unlocks_it() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.create_text_node([200.0, 0.0]).unwrap();
        doc.set_nodes_locked(&HashSet::from([0]), true).unwrap();
        let mut interaction = Interaction {
            selected: HashSet::from([0, 1]),
            ..default()
        };
        let mut editor = editor::Editor::default();
        for expected in [true, false] {
            execute(
                Command::ToggleLock,
                Vec2::ZERO,
                &mut doc,
                &mut interaction,
                &mut editor,
                &mut physics::PhysicsEditor::default(),
                &mut effects::Effects::default(),
            )
            .unwrap();
            assert!(doc.nodes.iter().all(|node| node.locked == expected));
        }
        doc.undo();
        assert!(doc.nodes.iter().all(|node| node.locked));
    }

    #[test]
    fn edit_command_opens_section_title_and_commit_preserves_bounds() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let group = doc.create_group(&HashSet::from([0])).unwrap();
        let bounds = doc.nodes[group].rect;
        let mut interaction = Interaction {
            selected: HashSet::from([group]),
            ..default()
        };
        let mut editor = editor::Editor::default();
        execute(
            Command::Edit,
            Vec2::ZERO,
            &mut doc,
            &mut interaction,
            &mut editor,
            &mut physics::PhysicsEditor::default(),
            &mut effects::Effects::default(),
        )
        .unwrap();
        assert!(editor.is_editing(&doc.nodes[group]));
        assert!(editor.commit(&mut doc));
        assert_eq!(doc.nodes[group].rect, bounds);
    }

    #[test]
    fn first_context_swatch_clears_color_and_is_undoable() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.nodes[0].color = [1.0, 0.0, 0.0, 1.0];
        let original = doc.nodes[0].color;
        let interaction = Interaction {
            selected: HashSet::from([0]),
            ..default()
        };
        let ctx = egui::Context::default();
        let mut tools = Tools {
            context: Some((egui::pos2(20.0, 20.0), Vec2::ZERO)),
            ..default()
        };
        let mut swatch = None;
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                tools.show(ui.ctx(), false, &doc, &interaction, true);
            });
            swatch = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.rect.size() == egui::vec2(24.0, 24.0) => {
                    Some(rect.rect.center())
                }
                _ => None,
            });
            output.textures_delta.clear();
        }
        let pos = swatch.expect("no-color swatch is rendered");
        let mut action = None;
        for pressed in [true, false] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..default()
                },
                |ui| {
                    action = tools.show(ui.ctx(), false, &doc, &interaction, true);
                },
            );
            output.textures_delta.clear();
        }
        let Some(Action::Apply(ids, properties)) = action else {
            panic!("swatch must apply a color");
        };
        assert_eq!(ids, HashSet::from([doc.nodes[0].id.clone()]));
        assert_eq!(properties.color, Some([0.0; 4]));
        doc.set_node_properties(&interaction.selected, &properties)
            .unwrap();
        assert_eq!(doc.nodes[0].color, [0.0; 4]);
        doc.undo();
        assert_eq!(doc.nodes[0].color, original);
    }

    fn frame(
        tools: &mut Tools,
        ctx: &egui::Context,
        doc: &Document,
        events: Vec<egui::Event>,
    ) -> Option<Action> {
        let mut action = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 400.0),
                )),
                events,
                ..default()
            },
            |_| {
                action = tools.show(ctx, false, doc, &Interaction::default(), true);
            },
        );
        output.textures_delta.clear();
        action
    }

    #[test]
    fn search_enter_focuses_a_result_but_not_during_ime() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.nodes[0].text = "中文节点".into();
        let ctx = egui::Context::default();
        let mut tools = Tools::default();
        tools.open_search();
        tools.query = "中文".into();
        frame(&mut tools, &ctx, &doc, vec![]);
        let enter = || egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        tools.search_composing = true;
        assert!(frame(&mut tools, &ctx, &doc, vec![enter()]).is_none());
        assert!(tools.search);
        assert!(
            frame(
                &mut tools,
                &ctx,
                &doc,
                vec![egui::Event::Ime(egui::ImeEvent::Commit(String::new()))]
            )
            .is_none()
        );
        tools.query = "中文".into();
        tools.focus_search = true;
        frame(&mut tools, &ctx, &doc, vec![]);
        assert!(matches!(
            frame(&mut tools, &ctx, &doc, vec![enter()]),
            Some(Action::Focus(0))
        ));
        assert!(!tools.search);
    }
}

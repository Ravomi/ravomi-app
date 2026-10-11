use super::{tabs::Spring, theme};
use bevy_egui::egui;

pub enum Action {
    Canvas(super::canvas_tools::Command),
    Home,
    New,
    Open,
    ImportImage,
    PasteImage,
    Recover,
    Export,
    Save,
    Undo,
    Redo,
    Fit,
    Connect,
    About,
    SaveCopy,
    SelectAll,
    Group,
    ToggleTheme,
    Zoom(f32),
    TogglePhysics,
    Help,
    ResetZoom,
    Origin,
    Fullscreen,
    Maximize,
    Minimize,
    Pin,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(menu: &mut Dropdown, ctx: &egui::Context, time: f64, events: Vec<egui::Event>) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                events,
                ..Default::default()
            },
            |_| {
                menu.show(ctx, false, true, true, true, false);
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn dropdown_from_rightmost_trigger_stays_inside_a_narrow_window() {
        for section in [0, 7] {
            let ctx = egui::Context::default();
            let mut menu = Dropdown {
                open: true,
                section,
                trigger: Some(egui::Rect::from_min_size(
                    egui::pos2(280.0, 16.0),
                    egui::vec2(24.0, 40.0),
                )),
                ..Default::default()
            };
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 300.0));
            for tick in 0..90 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        time: Some(tick as f64 / 60.0),
                        ..Default::default()
                    },
                    |_| {
                        menu.show(&ctx, false, true, true, true, false);
                    },
                );
                output.textures_delta.clear();
            }
            let panel = menu.panel.unwrap();
            assert!(screen.shrink(15.0).contains_rect(panel), "{panel:?}");
        }
    }

    #[test]
    fn escape_and_outside_press_close_but_keep_capture_during_animation() {
        for escape in [true, false] {
            let ctx = egui::Context::default();
            let mut menu = Dropdown {
                open: true,
                ..Default::default()
            };
            for tick in 0..60 {
                frame(&mut menu, &ctx, tick as f64 / 60.0, vec![]);
            }
            assert!(menu.open);
            let events = if escape {
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            } else {
                vec![
                    egui::Event::PointerMoved(egui::pos2(900.0, 800.0)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(900.0, 800.0),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            frame(&mut menu, &ctx, 1.0, events);
            assert!(!menu.open);
            assert!(menu.visible());
            for tick in 61..150 {
                frame(&mut menu, &ctx, tick as f64 / 60.0, vec![]);
            }
            assert!(!menu.visible());
        }
    }
}

#[derive(Default)]
pub struct Dropdown {
    pub open: bool,
    pub trigger: Option<egui::Rect>,
    panel: Option<egui::Rect>,
    pub section: usize,
    pub icons: Vec<egui::TextureHandle>,
    pub can_undo: bool,
    pub can_redo: bool,
    reveal: Spring,
    horizontal: Spring,
    height: Spring,
    content: Spring,
}

impl Dropdown {
    pub fn select(&mut self, section: usize) {
        if self.open && self.section == section {
            return;
        }
        self.open = true;
        self.section = section;
        self.content = Spring::default();
    }
    pub fn visible(&self) -> bool {
        self.open || self.reveal.value > 0.001
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        light: bool,
        can_save: bool,
        can_edit: bool,
        can_group: bool,
        pushing: bool,
    ) -> Option<Action> {
        if !self.visible() {
            return None;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.open = false;
        }
        if ctx.input(|i| i.pointer.any_pressed())
            && ctx.pointer_interact_pos().is_some_and(|p| {
                !self.trigger.is_some_and(|r| r.contains(p))
                    && !self.panel.is_some_and(|r| r.contains(p))
            })
        {
            self.open = false;
        }
        let dt = ctx.input(|i| i.stable_dt.min(0.05));
        let moving = self
            .reveal
            .step(if self.open { 1.0 } else { 0.0 }, dt * 0.5);
        let content_moving = self.content.step(1.0, dt * 1.5);
        let top = self.trigger.map_or(theme::DOCK_TOP + 56.0, |r| r.bottom());
        let target_height: f32 = match self.section {
            0 => 368.0,
            1 => 360.0,
            2 => 132.0,
            3 => 174.0,
            5 => 164.0,
            7 => 272.0,
            _ => 96.0,
        };
        let target_height = target_height.min((ctx.content_rect().bottom() - top - 32.0).max(80.0));
        if self.height.value == 0.0 {
            self.height.value = target_height;
        }
        let resizing = self.height.step(target_height, dt * 0.5);
        let width = 252.0_f32.min((ctx.content_rect().width() - 32.0).max(1.0));
        let min_left = ctx.content_rect().left() + 16.0;
        let max_left = (ctx.content_rect().right() - width - 16.0).max(min_left);
        let left = self
            .trigger
            .map_or(24.0, |r| r.left())
            .clamp(min_left, max_left);
        if self.horizontal.value == 0.0 {
            self.horizontal.value = left;
        }
        let sliding = self.horizontal.step(left, dt * 0.5);
        if moving || content_moving || resizing || sliding {
            ctx.request_repaint();
        }
        let progress = self.reveal.value.clamp(0.0, 1.0);
        let p = theme::chrome_palette(light);
        let mut action = None;
        let mut next_section = None;
        let area = egui::Area::new("dock-dropdown".into())
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(
                self.horizontal.value.clamp(min_left, max_left),
                top + 16.0 - (1.0 - progress) * 8.0,
            ))
            .show(ctx, |ui| {
                ui.set_opacity(progress);
                let height = (self.height.value * progress).max(1.0);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect.translate(egui::vec2(0.0, 4.0)),
                    18.0,
                    egui::Color32::from_black_alpha(if light { 10 } else { 30 }),
                );
                ui.painter().rect_filled(
                    rect,
                    18.0,
                    theme::egui_color(theme::palette(light).canvas),
                );
                ui.painter().rect_stroke(
                    rect,
                    18.0,
                    egui::Stroke::new(1.0, p.border),
                    egui::StrokeKind::Inside,
                );
                let inner = egui::Rect::from_min_size(
                    rect.min + egui::vec2(10.0, 10.0),
                    egui::vec2(width - 20.0, self.height.value - 20.0),
                );
                let mut body = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                body.set_clip_rect(
                    rect.shrink2(egui::vec2(10.0, 0.0))
                        .intersect(ui.clip_rect()),
                );
                if !self.open || progress <= 0.7 {
                    body.disable();
                }
                theme::chrome_style(&mut body, light);
                body.style_mut().override_font_id = Some(egui::FontId::proportional(15.0));
                body.spacing_mut().interact_size.y = 32.0;
                body.spacing_mut().item_spacing.y = 4.0;
                body.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                body.set_opacity(progress * self.content.value.clamp(0.0, 1.0));
                egui::ScrollArea::vertical()
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .max_height((self.height.value - 20.0).max(1.0))
                    .show(&mut body, |body| {
                        let mut row = |label: &str, shortcut: &str, enabled: bool| {
                            if label.is_empty() {
                                body.separator();
                                return false;
                            }
                            let icon = match label {
                                "新建草稿" => 18,
                                "打开文件…" => 8,
                                "最近打开" => 17,
                                "保存" => 0,
                                "另存为…" | "导出 PNG…" => 19,
                                "导入图片…" => 20,
                                "粘贴剪贴板图片" => 28,
                                "恢复副本…" => 2,
                                "撤销" => 1,
                                "重做" => 2,
                                "创建分组" | "全选" => 13,
                                "连线模式" => 4,
                                "查看全图" | "重置缩放" => 14,
                                "使用指南" => 21,
                                "关于 Ravomi" => 10,
                                _ => 9,
                            };
                            let button = if let Some(icon) = self.icons.get(icon) {
                                egui::Button::new((
                                    egui::Image::new(icon)
                                        .fit_to_exact_size(egui::vec2(18.0, 18.0))
                                        .tint(p.foreground),
                                    label,
                                ))
                            } else {
                                egui::Button::new(label)
                            };
                            body.add_enabled(
                                enabled,
                                button
                                    .truncate()
                                    .shortcut_text(shortcut)
                                    .frame(false)
                                    .min_size(egui::vec2(body.available_width(), 32.0)),
                            )
                            .clicked()
                        };
                        match self.section {
                            7 => {
                                if row("返回启动页", "", true) {
                                    action = Some(Action::Home);
                                }
                                for (label, section) in [
                                    ("文件", 0),
                                    ("编辑", 1),
                                    ("视图", 3),
                                    ("设置", 2),
                                    ("窗口", 5),
                                    ("帮助与关于", 4),
                                ] {
                                    if row(label, "›", true) {
                                        next_section = Some(section);
                                    }
                                }
                            }
                            0 => {
                                if row("新建草稿", "", can_save) {
                                    action = Some(Action::New);
                                }
                                if row("打开文件…", "", can_save) {
                                    action = Some(Action::Open);
                                }
                                if row("最近打开", "", true) {
                                    action = Some(Action::Home);
                                }
                                row("", "", false);
                                if row("保存", "Ctrl+S", can_save) {
                                    action = Some(Action::Save);
                                }
                                if row("另存为…", "", can_save) {
                                    action = Some(Action::SaveCopy);
                                }
                                row("", "", false);
                                if row("导入图片…", "", can_save && can_edit) {
                                    action = Some(Action::ImportImage);
                                }
                                if row("粘贴剪贴板图片", "", can_save && can_edit) {
                                    action = Some(Action::PasteImage);
                                }
                                if row("恢复副本…", "", can_save) {
                                    action = Some(Action::Recover);
                                }
                                if row("导出 PNG…", "", can_save && can_edit) {
                                    action = Some(Action::Export);
                                }
                            }
                            1 => {
                                if row("新建节点", "双击", can_edit) {
                                    action =
                                        Some(Action::Canvas(super::canvas_tools::Command::NewNode));
                                }
                                if row("搜索当前图", "Ctrl+F", can_edit) {
                                    action =
                                        Some(Action::Canvas(super::canvas_tools::Command::Search));
                                }
                                if row("节点属性", "Alt+Enter", can_group) {
                                    action = Some(Action::Canvas(
                                        super::canvas_tools::Command::Properties,
                                    ));
                                }
                                if row("撤销", "Ctrl+Z", can_edit && self.can_undo) {
                                    action = Some(Action::Undo);
                                }
                                if row("重做", "Ctrl+Shift+Z", can_edit && self.can_redo) {
                                    action = Some(Action::Redo);
                                }
                                if row("连线模式", "E", can_edit) {
                                    action = Some(Action::Connect);
                                }
                                row("", "", false);
                                if row("全选", "Ctrl+A", can_edit) {
                                    action = Some(Action::SelectAll);
                                }
                                if row("创建分组", "Ctrl+G", can_group) {
                                    action = Some(Action::Group);
                                }
                                if row(
                                    "节点推挤",
                                    if pushing { "已开启" } else { "已关闭" },
                                    can_edit,
                                ) {
                                    action = Some(Action::TogglePhysics);
                                }
                            }
                            2 => {
                                if row(
                                    if light {
                                        "切换为深色模式"
                                    } else {
                                        "切换为浅色模式"
                                    },
                                    "",
                                    true,
                                ) {
                                    action = Some(Action::ToggleTheme);
                                }
                                body.add_space(8.0);
                                body.label(egui::RichText::new("界面缩放").small().color(p.muted));
                                body.horizontal(|ui| {
                                    for factor in [1.0, 1.5, 2.0] {
                                        if ui
                                            .add_sized(
                                                [(width - 32.0) / 3.0, 36.0],
                                                egui::Button::new(format!(
                                                    "{:.0}%",
                                                    factor * 100.0
                                                ))
                                                .selected(ctx.zoom_factor() == factor),
                                            )
                                            .clicked()
                                        {
                                            action = Some(Action::Zoom(factor));
                                        }
                                    }
                                });
                            }
                            3 => {
                                if row("查看全图", "F", can_edit) {
                                    action = Some(Action::Fit);
                                }
                                if row("返回启动页", "", true) {
                                    action = Some(Action::Home);
                                }
                                row("", "", false);
                                if row("重置缩放", "100%", can_edit) {
                                    action = Some(Action::ResetZoom);
                                }
                                if row("移到坐标原点", "", can_edit) {
                                    action = Some(Action::Origin);
                                }
                            }
                            5 => {
                                if row("窗口置顶 / 取消置顶", "", true) {
                                    action = Some(Action::Pin);
                                }
                                if row("切换全屏", "", true) {
                                    action = Some(Action::Fullscreen);
                                }
                                if row("最大化 / 还原", "", true) {
                                    action = Some(Action::Maximize);
                                }
                                if row("最小化", "", true) {
                                    action = Some(Action::Minimize);
                                }
                            }
                            _ => {
                                if row("使用指南", "", true) {
                                    action = Some(Action::Help);
                                }
                                if row("关于 Ravomi", "", true) {
                                    action = Some(Action::About);
                                }
                            }
                        }
                    });
            });
        self.panel = Some(area.response.rect);
        if let Some(section) = next_section {
            self.select(section);
        }
        if action.is_some()
            && !matches!(
                action,
                Some(Action::ToggleTheme | Action::Zoom(_) | Action::TogglePhysics)
            )
        {
            self.open = false;
        }
        action
    }
}

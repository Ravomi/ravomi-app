use super::{Document, Graph, theme};
use bevy_egui::egui;
use project_graph_bevy_canvas::storage;
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, mpsc},
};

pub enum Action {
    New,
    Open,
    Recent(PathBuf),
    Settings,
    Help,
    About,
    Menu(egui::Rect),
    CloseHome,
    Minimize,
    Maximize,
    CloseWindow,
    DragWindow,
    RemoveRecent { path: PathBuf, delete_file: bool },
    Error(String),
}

struct DeleteRequest {
    path: PathBuf,
    delete_file: bool,
}

struct Recent {
    path: PathBuf,
    preview: Option<Graph>,
    error: Option<String>,
    modified: Option<std::time::SystemTime>,
}

#[derive(Default)]
pub struct Home {
    pub visible: bool,
    pub icons: Vec<egui::TextureHandle>,
    search: String,
    list_view: bool,
    focus_search: bool,
    recent: Vec<Recent>,
    observed: Option<PathBuf>,
    loaded: bool,
    revision: Option<u64>,
    was_visible: bool,
    deletion: Option<DeleteRequest>,
    location: Option<Mutex<mpsc::Receiver<Result<(), String>>>>,
    avatar: Option<egui::TextureHandle>,
}

fn nav_button(
    icons: &[egui::TextureHandle],
    index: usize,
    label: &str,
    color: egui::Color32,
    font_size: f32,
) -> egui::Button<'static> {
    let text = egui::RichText::new(label).size(font_size).color(color);
    if let Some(icon) = icons.get(index) {
        egui::Button::new((
            egui::Image::new((icon.id(), egui::vec2(17.0, 17.0))).tint(color),
            text,
            egui::Atom::grow(),
        ))
        .gap(10.0)
        .corner_radius(12)
    } else {
        egui::Button::new(text).corner_radius(12)
    }
}

impl Home {
    pub fn observe(&mut self, document: &Document) -> Result<(), String> {
        self.observe_at(document, storage::recent_path())
    }

    fn observe_at(&mut self, document: &Document, storage: Option<PathBuf>) -> Result<(), String> {
        let refresh = !self.loaded || (self.visible && !self.was_visible);
        self.was_visible = self.visible;
        let mut changed = false;
        if !self.loaded {
            self.loaded = true;
            if let Some(path) = &storage {
                match std::fs::read(path) {
                    Ok(bytes) => {
                        let paths: Vec<PathBuf> = serde_json::from_slice(&bytes)
                            .map_err(|e| format!("读取最近文件记录失败：{e}"))?;
                        self.recent = paths
                            .into_iter()
                            .take(30)
                            .map(|path| Recent {
                                modified: file_modified(&path),
                                path,
                                preview: None,
                                error: None,
                            })
                            .collect();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("读取最近文件记录失败：{e}")),
                }
            }
        }
        if refresh {
            let previous = self.recent.len();
            self.recent
                .retain(|recent| match std::fs::metadata(&recent.path) {
                    Ok(metadata) => metadata.is_file(),
                    Err(error) => error.kind() != std::io::ErrorKind::NotFound,
                });
            changed = previous != self.recent.len();
        }
        if self.observed != document.path {
            self.observed = document.path.clone();
            self.revision = None;
            if let Some(path) = &document.path
                && path.is_file()
            {
                self.recent.retain(|r| &r.path != path);
                self.recent.insert(
                    0,
                    Recent {
                        modified: file_modified(path),
                        path: path.clone(),
                        preview: None,
                        error: None,
                    },
                );
                self.recent.truncate(30);
                changed = true;
            }
        }
        if changed {
            self.persist(storage.as_deref())?;
        }
        if self.revision != Some(document.revision) {
            self.revision = Some(document.revision);
            if let Some(recent) = self
                .recent
                .iter_mut()
                .find(|recent| Some(&recent.path) == document.path.as_ref())
            {
                recent.modified = file_modified(&recent.path);
                recent.preview = Some(Graph {
                    nodes: document.nodes.clone(),
                    edges: document.edges.clone(),
                    attachments: Default::default(),
                    counts: document.counts.clone(),
                });
                recent.error = None;
            }
        }
        Ok(())
    }

    fn persist(&self, storage: Option<&std::path::Path>) -> Result<(), String> {
        let paths: Vec<_> = self.recent.iter().map(|r| &r.path).collect();
        Self::persist_paths(&paths, storage)
    }

    fn persist_paths(paths: &[&PathBuf], storage: Option<&Path>) -> Result<(), String> {
        let Some(destination) = storage else {
            return Ok(());
        };
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let parent = destination.parent().ok_or("最近文件记录缺少目录")?;
            std::fs::create_dir_all(parent)?;
            storage::atomic_write(destination, |file| {
                serde_json::to_writer(file, &paths)?;
                Ok(())
            })
        };
        write().map_err(|error| format!("保存最近文件记录失败：{error}"))
    }

    pub fn remove_recent(&mut self, path: &Path, delete_file: bool) -> Result<(), String> {
        self.remove_recent_at(path, delete_file, storage::recent_path().as_deref())
    }

    fn remove_recent_at(
        &mut self,
        path: &Path,
        delete_file: bool,
        storage: Option<&Path>,
    ) -> Result<(), String> {
        if !self.recent.iter().any(|recent| recent.path == path) {
            return Err("该文件已不在最近列表中".into());
        }
        if delete_file {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("删除本机文件失败：{}\n{error}", path.display())),
            }
        }
        let paths: Vec<_> = self
            .recent
            .iter()
            .filter(|recent| recent.path != path)
            .map(|recent| &recent.path)
            .collect();
        Self::persist_paths(&paths, storage).map_err(|error| {
            if delete_file {
                format!("本机文件已删除，但最近列表未保存：{error}")
            } else {
                error
            }
        })?;
        self.recent.retain(|recent| recent.path != path);
        Ok(())
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        _light: bool,
        is_open: impl Fn(&Path) -> bool,
    ) -> Option<Action> {
        if let Some(receiver) = &self.location {
            let result = match receiver.lock() {
                Ok(receiver) => match receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("打开文件位置的任务中断".into()))
                    }
                },
                Err(_) => Some(Err("打开文件位置的任务状态损坏".into())),
            };
            if let Some(result) = result {
                self.location = None;
                if let Err(error) = result {
                    return Some(Action::Error(error));
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        if !self.visible {
            return None;
        }
        let light = false;
        if self.avatar.is_none() {
            match image::load_from_memory(include_bytes!(
                "../assets/branding/ravomi-avatar-cutout.png"
            )) {
                Ok(image) => {
                    let image = image.to_rgba8();
                    self.avatar = Some(ctx.load_texture(
                        "ravomi-avatar",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    ));
                }
                Err(error) => return Some(Action::Error(format!("品牌图像加载失败：{error}"))),
            }
        }
        let colors = theme::home_palette();
        let accent = colors.active;
        let screen = ctx.content_rect();
        let header_height = 42.0_f32.max(theme::window_chrome_height(ctx));
        let sidebar_width = if screen.width() < 700.0 { 176.0 } else { 200.0 };
        let rect = egui::Rect::from_min_max(egui::pos2(0.0, header_height), screen.max);
        let mut action = None;
        let mut reveal = None;
        egui::Area::new("home".into())
            .fixed_pos(rect.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_visuals(egui::Visuals::dark());
                ui.set_min_size(rect.size());
                ui.painter().rect_filled(screen, 0.0, colors.surface);
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(egui::pos2(sidebar_width, rect.top()), rect.max),
                    12.0,
                    colors.ink,
                );
                ui.painter().rect_stroke(
                    egui::Rect::from_min_max(egui::pos2(sidebar_width, rect.top()), rect.max),
                    12.0,
                    egui::Stroke::new(1.0, colors.border),
                    egui::StrokeKind::Inside,
                );
                let sidebar = egui::Rect::from_min_max(
                    rect.min + egui::vec2(20.0, 24.0),
                    egui::pos2(sidebar_width - 16.0, rect.bottom() - 16.0),
                );
                let mut nav = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(sidebar)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                egui::ScrollArea::vertical()
                    .id_salt("home-navigation")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .max_height(sidebar.height())
                    .auto_shrink([false, false])
                    .show(&mut nav, |nav| {
                        theme::chrome_style(nav, light);
                        nav.spacing_mut().interact_size.y = 38.0;
                        nav.spacing_mut().item_spacing = egui::vec2(10.0, 5.0);
                        nav.spacing_mut().button_padding = egui::vec2(14.0, 6.0);
                        let visuals = nav.visuals_mut();
                        visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                        visuals.widgets.hovered.weak_bg_fill = accent.gamma_multiply(0.08);
                        visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.14);
                        visuals.selection.bg_fill = colors.border;
                        visuals.selection.stroke = egui::Stroke::NONE;
                        nav.horizontal(|ui| {
                            if let Some(avatar) = &self.avatar {
                                ui.add(egui::Image::new((avatar.id(), egui::vec2(46.0, 46.0))));
                            }
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new("Ravomi")
                                        .size(20.0)
                                        .color(colors.foreground),
                                );
                                ui.label(
                                    egui::RichText::new("人和 Agent，共绘工作")
                                        .size(10.0)
                                        .color(colors.muted),
                                );
                            });
                        });
                        nav.add_space(22.0);
                        let width = nav.available_width();
                        if nav
                            .add_sized(
                                [width, 40.0],
                                nav_button(&self.icons, 18, "新建画布", colors.on_primary, 14.0)
                                    .frame(true)
                                    .fill(colors.primary)
                                    .corner_radius(12),
                            )
                            .clicked()
                        {
                            action = Some(Action::New);
                        }
                        if nav
                            .add_sized(
                                [width, 38.0],
                                nav_button(&self.icons, 16, "打开文件…", colors.foreground, 13.0),
                            )
                            .clicked()
                        {
                            action = Some(Action::Open);
                        }
                        nav.add_sized(
                            [width, 38.0],
                            nav_button(&self.icons, 17, "最近打开", colors.foreground, 13.0)
                                .frame(true)
                                .selected(true)
                                .corner_radius(12),
                        );
                        if nav
                            .add_sized(
                                [width, 38.0],
                                nav_button(&self.icons, 21, "使用指南", colors.foreground, 13.0),
                            )
                            .clicked()
                        {
                            action = Some(Action::Help);
                        }
                        nav.add_space((sidebar.bottom() - nav.cursor().top() - 180.0).max(12.0));
                        nav.separator();
                        nav.add_space(8.0);
                        if nav
                            .add_sized(
                                [width, 32.0],
                                nav_button(&self.icons, 9, "设置", colors.foreground, 12.0),
                            )
                            .clicked()
                        {
                            action = Some(Action::Settings);
                        }
                        if nav
                            .add_sized(
                                [width, 32.0],
                                nav_button(&self.icons, 25, "关于", colors.foreground, 12.0),
                            )
                            .clicked()
                        {
                            action = Some(Action::About);
                        }
                        if nav
                            .add_sized(
                                [width, 32.0],
                                nav_button(&self.icons, 26, "项目仓库", colors.foreground, 12.0),
                            )
                            .clicked()
                        {
                            ctx.open_url(egui::OpenUrl::new_tab(
                                "https://github.com/Ravomi/ravomi-app",
                            ));
                        }
                        nav.add_space(8.0);
                        nav.label(
                            egui::RichText::new("Arcraven")
                                .size(10.0)
                                .color(colors.muted),
                        );
                    });

                let content = egui::Rect::from_min_max(
                    egui::pos2(sidebar_width + 30.0, rect.top() + 28.0),
                    rect.max - egui::vec2(32.0, 20.0),
                );
                let mut body = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(content)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                theme::chrome_style(&mut body, light);
                body.spacing_mut().interact_size.y = 18.0;
                body.spacing_mut().item_spacing.y = 4.0;
                let compact = content.width() < 520.0 || content.height() < 450.0;
                let (hero, _) = body.allocate_exact_size(
                    egui::vec2(content.width(), if compact { 108.0 } else { 132.0 }),
                    egui::Sense::hover(),
                );
                let painter = body.painter();
                painter.text(
                    hero.min + egui::vec2(12.0, 20.0),
                    egui::Align2::LEFT_CENTER,
                    "你的工作空间",
                    egui::FontId::proportional(12.0),
                    colors.muted,
                );
                painter.text(
                    hero.min + egui::vec2(12.0, 64.0),
                    egui::Align2::LEFT_CENTER,
                    "让灵感安静生长",
                    egui::FontId::proportional(if compact { 26.0 } else { 36.0 }),
                    colors.foreground,
                );
                if !compact {
                    painter.text(
                        hero.min + egui::vec2(12.0, 106.0),
                        egui::Align2::LEFT_CENTER,
                        "记录、连接，逐步展开你的工作。",
                        egui::FontId::proportional(17.0),
                        colors.muted,
                    );
                    if let Some(avatar) = &self.avatar {
                        let image = egui::Rect::from_min_size(
                            egui::pos2(hero.right() - 150.0, hero.top()),
                            egui::vec2(136.0, 136.0),
                        );
                        painter.image(
                            avatar.id(),
                            image,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                }
                body.separator();
                body.add_space(16.0);
                body.horizontal(|ui| {
                    ui.label(egui::RichText::new("我的画布").size(23.0).strong());
                    ui.label(
                        egui::RichText::new(self.recent.len().to_string())
                            .size(17.0)
                            .color(colors.muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if view_button(ui, true, self.list_view).clicked() {
                            self.list_view = true;
                        }
                        if view_button(ui, false, !self.list_view).clicked() {
                            self.list_view = false;
                        }
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("搜索最近文件")
                                .desired_width(if content.width() > 700.0 {
                                    170.0
                                } else {
                                    130.0
                                })
                                .frame(
                                    egui::Frame::new()
                                        .fill(colors.surface)
                                        .inner_margin(10.0)
                                        .corner_radius(12),
                                ),
                        );
                        if self.focus_search {
                            search.request_focus();
                            self.focus_search = false;
                        }
                    });
                });
                body.add_space(6.0);
                body.label(
                    egui::RichText::new("最近打开")
                        .size(13.0)
                        .color(colors.muted),
                );
                body.add_space(10.0);
                let query = self.search.trim().to_lowercase();
                let columns = if self.list_view || content.width() < 620.0 {
                    1
                } else {
                    2
                };
                let card_width = (content.width() - (columns - 1) as f32 * 16.0) / columns as f32;
                egui::ScrollArea::vertical()
                    .max_height((content.bottom() - body.cursor().top() - 24.0).max(40.0))
                    .show(&mut body, |ui| {
                        let matches: Vec<_> = self
                            .recent
                            .iter_mut()
                            .filter(|r| r.path.to_string_lossy().to_lowercase().contains(&query))
                            .collect();
                        if matches.is_empty() {
                            ui.add_space(48.0);
                            ui.label(if query.is_empty() {
                                "还没有最近文件。新建草稿，或打开 .mio 项目开始。"
                            } else {
                                "没有匹配的最近文件"
                            });
                        }
                        egui::Grid::new("recent-grid")
                            .spacing([16.0, 16.0])
                            .show(ui, |ui| {
                                for (index, recent) in matches.into_iter().enumerate() {
                                    if recent.preview.is_none() && recent.error.is_none() {
                                        match Document::open(&recent.path) {
                                            Ok(doc) => recent.preview = Some(doc.graph),
                                            Err(error) => recent.error = Some(error.to_string()),
                                        }
                                    }
                                    let (card, _) = ui.allocate_exact_size(
                                        egui::vec2(
                                            card_width,
                                            if self.list_view {
                                                76.0
                                            } else {
                                                (card_width * 0.42).clamp(180.0, 250.0)
                                            },
                                        ),
                                        egui::Sense::hover(),
                                    );
                                    let response = ui
                                        .interact(
                                            card,
                                            egui::Id::new(("recent-file", &recent.path)),
                                            egui::Sense::click(),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    recent_card(
                                        ui,
                                        card,
                                        recent,
                                        response.hovered() || response.has_focus(),
                                        self.list_view,
                                    );
                                    let menu_rect = egui::Rect::from_min_size(
                                        egui::pos2(card.right() - 40.0, card.bottom() - 58.0),
                                        egui::vec2(28.0, 28.0),
                                    );
                                    let more = ui
                                        .interact(
                                            menu_rect,
                                            egui::Id::new(("recent-menu", &recent.path)),
                                            egui::Sense::click(),
                                        )
                                        .on_hover_text("画布操作");
                                    more.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            true,
                                            "画布操作",
                                        )
                                    });
                                    if more.hovered() || more.has_focus() {
                                        ui.painter().rect_filled(menu_rect, 6.0, colors.border);
                                    }
                                    ui.painter().text(
                                        menu_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "⋯",
                                        egui::FontId::proportional(16.0),
                                        colors.foreground,
                                    );
                                    let on_menu = ui
                                        .input(|i| i.pointer.interact_pos())
                                        .is_some_and(|p| menu_rect.contains(p));
                                    if response.clicked() && !on_menu {
                                        action = Some(Action::Recent(recent.path.clone()));
                                    }
                                    let popup = egui::Popup::menu(&more);
                                    let popup = if response.secondary_clicked() {
                                        popup.open_memory(egui::SetOpenCommand::Bool(true))
                                    } else {
                                        popup
                                    };
                                    popup.frame(theme::tool_frame(light).inner_margin(6)).show(
                                        |ui| {
                                            theme::tool_style(ui, light);
                                            ui.spacing_mut().item_spacing.y = 2.0;
                                            if ui
                                                .add_enabled(
                                                    self.location.is_none(),
                                                    egui::Button::new("打开文件所在位置"),
                                                )
                                                .clicked()
                                            {
                                                reveal = Some(recent.path.clone());
                                                ui.close();
                                            }
                                            if ui.button("删除…").clicked() {
                                                self.deletion = Some(DeleteRequest {
                                                    path: recent.path.clone(),
                                                    delete_file: false,
                                                });
                                                ui.close();
                                            }
                                        },
                                    );
                                    response.on_hover_text(recent.error.as_deref().map_or_else(
                                        || recent.path.display().to_string(),
                                        |e| format!("{}\n{e}", recent.path.display()),
                                    ));
                                    if (index + 1) % columns == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                body.add_space(12.0);
                body.label(
                    egui::RichText::new("Ravomi · 让工作安静生长")
                        .size(11.0)
                        .color(colors.muted),
                );
            });
        if let Some(header_action) = self.header(ctx, sidebar_width, header_height) {
            action = Some(header_action);
        }
        if let Some(path) = reveal {
            let (sender, receiver) = mpsc::channel();
            self.location = Some(Mutex::new(receiver));
            std::thread::spawn(move || {
                let result = location_command(&path).and_then(|mut command| {
                    let output = command
                        .output()
                        .map_err(|error| format!("无法启动文件管理器：{error}"))?;
                    if output.status.success() {
                        Ok(())
                    } else {
                        Err(format!(
                            "打开文件所在位置失败：{}",
                            String::from_utf8_lossy(&output.stderr).trim()
                        ))
                    }
                });
                let _ = sender.send(result); // The app may have closed.
            });
            ctx.request_repaint();
        }
        if let Some(request) = &mut self.deletion {
            let mut dismiss = false;
            let response = egui::Modal::new("remove-recent-file".into())
                .frame(theme::tool_frame(light))
                .show(ctx, |ui| {
                    theme::tool_style(ui, light);
                    ui.set_width(360.0_f32.min(ctx.content_rect().width() - 64.0));
                    dismiss = theme::tool_header(ui, "删除最近文件", light);
                    ui.label(
                        request
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    );
                    ui.weak(request.path.display().to_string());
                    let opened = is_open(&request.path);
                    if opened {
                        request.delete_file = false;
                    }
                    ui.add_enabled(
                        !opened,
                        egui::Checkbox::new(
                            &mut request.delete_file,
                            "同时删除本机文件（永久删除）",
                        ),
                    );
                    ui.weak(if opened {
                        "文件已在标签页中打开。关闭后可删除本机文件。"
                    } else if request.delete_file {
                        "文件将从磁盘永久删除，无法撤销。"
                    } else {
                        "仅从最近列表移除，保留本机文件。"
                    });
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            dismiss = true;
                        }
                        if ui
                            .add(theme::primary_button(
                                if request.delete_file {
                                    "删除本机文件"
                                } else {
                                    "移除记录"
                                },
                                light,
                            ))
                            .clicked()
                        {
                            action = Some(Action::RemoveRecent {
                                path: request.path.clone(),
                                delete_file: request.delete_file,
                            });
                            dismiss = true;
                        }
                    });
                });
            if dismiss || response.should_close() {
                self.deletion = None;
            }
        }
        action
    }

    fn header(&mut self, ctx: &egui::Context, sidebar_width: f32, height: f32) -> Option<Action> {
        let colors = theme::home_palette();
        let mut action = None;
        egui::Area::new("home-header".into())
            .order(egui::Order::Foreground)
            .fixed_pos(egui::Pos2::ZERO)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_visuals(egui::Visuals::dark());
                theme::chrome_style(ui, false);
                ui.spacing_mut().interact_size.y = 24.0;
                let width = ctx.content_rect().width();
                ui.set_min_size(egui::vec2(width, height));
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height)),
                    0.0,
                    colors.surface,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(sidebar_width, 0.0),
                        egui::pos2(sidebar_width, height),
                    ],
                    egui::Stroke::new(1.0, colors.border),
                );
                if !cfg!(target_os = "macos") {
                    for (index, (label, color, tip)) in [
                        ("×", egui::Color32::from_rgb(237, 106, 95), "关闭窗口"),
                        ("−", egui::Color32::from_rgb(244, 191, 79), "最小化"),
                        ("+", egui::Color32::from_rgb(98, 197, 84), "最大化 / 还原"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let rect = egui::Rect::from_center_size(
                            egui::pos2(25.0 + index as f32 * 18.0, height * 0.5),
                            egui::vec2(13.0, 13.0),
                        );
                        let response = ui
                            .interact(
                                rect.expand(3.0),
                                egui::Id::new(("home-window", index)),
                                egui::Sense::click(),
                            )
                            .on_hover_text(tip);
                        ui.painter().circle_filled(rect.center(), 5.5, color);
                        if response.hovered() || response.has_focus() {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                label,
                                egui::FontId::proportional(10.0),
                                colors.ink,
                            );
                        }
                        if response.clicked() {
                            action = Some(match index {
                                0 => Action::CloseWindow,
                                1 => Action::Minimize,
                                _ => Action::Maximize,
                            });
                        }
                    }
                }
                let tab = egui::Rect::from_min_size(
                    egui::pos2(sidebar_width + 12.0, 9.0),
                    egui::vec2(120.0, height - 9.0),
                );
                let drag_rect = egui::Rect::from_min_max(
                    egui::pos2(tab.right() + 44.0, 0.0),
                    egui::pos2((width - 88.0).max(tab.right() + 44.0), height),
                );
                let drag = ui.interact(
                    drag_rect,
                    egui::Id::new("home-window-drag"),
                    egui::Sense::click_and_drag(),
                );
                if drag.double_clicked() {
                    action = Some(Action::Maximize);
                } else if drag.drag_started() {
                    action = Some(Action::DragWindow);
                }
                ui.painter().rect_filled(
                    tab,
                    egui::CornerRadius {
                        nw: 10,
                        ne: 10,
                        sw: 0,
                        se: 0,
                    },
                    colors.border,
                );
                ui.painter().text(
                    tab.left_center() + egui::vec2(18.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "首页",
                    egui::FontId::proportional(13.0),
                    colors.active,
                );
                if ui
                    .put(
                        egui::Rect::from_center_size(
                            tab.right_center() - egui::vec2(16.0, 0.0),
                            egui::vec2(24.0, 24.0),
                        ),
                        egui::Button::new("×").frame(false),
                    )
                    .on_hover_text("返回画布")
                    .clicked()
                {
                    action = Some(Action::CloseHome);
                }
                if ui
                    .put(
                        egui::Rect::from_min_size(
                            egui::pos2(tab.right() + 8.0, 8.0),
                            egui::vec2(30.0, 30.0),
                        ),
                        egui::Button::new("+").frame(false),
                    )
                    .on_hover_text("新建画布")
                    .clicked()
                {
                    action = Some(Action::New);
                }
                let menu = ui
                    .put(
                        egui::Rect::from_min_size(
                            egui::pos2(width - 82.0, 6.0),
                            egui::vec2(30.0, 30.0),
                        ),
                        egui::Button::new("⋯").frame(false),
                    )
                    .on_hover_text("菜单");
                if menu.clicked() {
                    action = Some(Action::Menu(menu.rect));
                }
                if ui
                    .put(
                        egui::Rect::from_min_size(
                            egui::pos2(width - 44.0, 6.0),
                            egui::vec2(30.0, 30.0),
                        ),
                        nav_button(&self.icons, 22, "", colors.foreground, 13.0).frame(false),
                    )
                    .on_hover_text("搜索最近文件")
                    .clicked()
                {
                    self.focus_search = true;
                }
            });
        action
    }
}

fn file_modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn view_button(ui: &mut egui::Ui, list: bool, selected: bool) -> egui::Response {
    let response = ui
        .add(
            egui::Button::new("")
                .selected(selected)
                .min_size(egui::vec2(32.0, 36.0)),
        )
        .on_hover_text(if list { "列表视图" } else { "网格视图" });
    let center = response.rect.center();
    let color = theme::home_palette().foreground;
    if list {
        for y in [-5.0, 0.0, 5.0] {
            ui.painter().line_segment(
                [center + egui::vec2(-6.0, y), center + egui::vec2(6.0, y)],
                egui::Stroke::new(1.2, color),
            );
        }
    } else {
        for x in [-4.0, 4.0] {
            for y in [-4.0, 4.0] {
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(center + egui::vec2(x, y), egui::vec2(6.0, 6.0)),
                    1.0,
                    color,
                );
            }
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            if list { "列表视图" } else { "网格视图" },
        )
    });
    response
}

fn location_command(path: &Path) -> Result<std::process::Command, String> {
    let parent = path
        .parent()
        .ok_or("文件缺少所在目录")?
        .canonicalize()
        .map_err(|error| format!("无法访问文件所在目录：{error}"))?;
    #[cfg(target_os = "macos")]
    let command = {
        let mut command = std::process::Command::new("open");
        command.arg(parent);
        command
    };
    #[cfg(target_os = "windows")]
    let command = {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(parent);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(parent);
        command
    };
    Ok(command)
}

fn recent_card(ui: &egui::Ui, card: egui::Rect, recent: &Recent, highlighted: bool, list: bool) {
    let colors = theme::home_palette();
    let painter = ui.painter().with_clip_rect(card.intersect(ui.clip_rect()));
    let inner = card.shrink(1.0);
    painter.rect_filled(inner, 12.0, colors.surface);
    painter.rect_stroke(
        inner,
        12.0,
        egui::Stroke::new(
            1.0,
            if highlighted {
                colors.active
            } else {
                colors.border
            },
        ),
        egui::StrokeKind::Inside,
    );
    let sheet = if list {
        egui::Rect::from_min_size(
            inner.min + egui::vec2(9.0, 9.0),
            egui::vec2(112.0, inner.height() - 18.0),
        )
    } else {
        egui::Rect::from_min_max(
            inner.min + egui::vec2(10.0, 10.0),
            egui::pos2(inner.right() - 10.0, inner.bottom() - 58.0),
        )
    };
    painter.rect_filled(sheet, 6.0, colors.ink);
    for y in 0..(sheet.height() / 12.0) as usize {
        for x in 0..(sheet.width() / 12.0) as usize {
            painter.circle_filled(
                sheet.min + egui::vec2(6.0 + x as f32 * 12.0, 6.0 + y as f32 * 12.0),
                0.55,
                colors.border.gamma_multiply(0.65),
            );
        }
    }
    if let Some(graph) = &recent.preview {
        if graph.nodes.is_empty() {
            painter.text(
                sheet.center(),
                egui::Align2::CENTER_CENTER,
                "空白画布",
                egui::FontId::proportional(14.0),
                colors.muted,
            );
        } else {
            preview(
                &painter.with_clip_rect(sheet.intersect(painter.clip_rect())),
                sheet.shrink2(egui::vec2(30.0, 8.0)),
                graph,
                colors.foreground,
            );
        }
    }
    let name = recent
        .path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let mut job = egui::text::LayoutJob::simple(
        name.into_owned(),
        egui::FontId::proportional(16.0),
        colors.foreground,
        (inner.width() - if list { 210.0 } else { 66.0 }).max(1.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    painter.galley(
        if list {
            inner.min + egui::vec2(136.0, 17.0)
        } else {
            inner.left_bottom() + egui::vec2(14.0, -47.0)
        },
        painter.layout_job(job),
        colors.foreground,
    );
    let detail = if recent.error.is_some() {
        "文件不可用".to_owned()
    } else if let Some(graph) = &recent.preview {
        format!(
            "{} 个节点 · {} 条连接",
            graph.nodes.len(),
            graph.edges.len()
        )
    } else {
        "Ravomi 项目".to_owned()
    };
    painter.text(
        inner.left_bottom() + egui::vec2(if list { 136.0 } else { 14.0 }, -17.0),
        egui::Align2::LEFT_CENTER,
        detail,
        egui::FontId::proportional(12.0),
        colors.muted,
    );
    if let Some(modified) = recent.modified {
        let seconds = modified.elapsed().unwrap_or_default().as_secs();
        let date = if seconds < 60 {
            "刚刚修改".into()
        } else if seconds < 3600 {
            format!("{} 分钟前", seconds / 60)
        } else if seconds < 86400 {
            format!("{} 小时前", seconds / 3600)
        } else {
            format!("{} 天前", seconds / 86400)
        };
        painter.text(
            inner.right_bottom() + egui::vec2(-14.0, -17.0),
            egui::Align2::RIGHT_CENTER,
            date,
            egui::FontId::proportional(12.0),
            colors.muted,
        );
    }
}

fn preview(painter: &egui::Painter, rect: egui::Rect, graph: &Graph, color: egui::Color32) {
    if graph.nodes.is_empty() {
        return;
    }
    let [x, y, w, h] = graph.bounds();
    let scale = (rect.width() / w.max(1.0)).min(rect.height() / h.max(1.0));
    let origin = rect.center() - egui::vec2(w, h) * scale * 0.5;
    let point = |px, py| origin + egui::vec2(px - x, py - y) * scale;
    for edge in &graph.edges {
        for pair in edge.members.windows(2) {
            if let (Some(a), Some(b)) = (graph.nodes.get(pair[0]), graph.nodes.get(pair[1])) {
                painter.line_segment(
                    [
                        point(a.rect[0] + a.rect[2] * 0.5, a.rect[1] + a.rect[3] * 0.5),
                        point(b.rect[0] + b.rect[2] * 0.5, b.rect[1] + b.rect[3] * 0.5),
                    ],
                    egui::Stroke::new(0.6, color.gamma_multiply(0.45)),
                );
            }
        }
    }
    for node in &graph.nodes {
        let r = egui::Rect::from_min_size(
            point(node.rect[0], node.rect[1]),
            egui::vec2(node.rect[2], node.rect[3]) * scale,
        );
        painter.rect_filled(r, 2.0, theme::home_palette().surface);
        painter.rect_stroke(
            r,
            2.0,
            egui::Stroke::new(0.7, color.gamma_multiply(0.65)),
            egui::StrokeKind::Inside,
        );
        if r.width() > 12.0 && r.height() > 7.0 {
            painter.with_clip_rect(r.intersect(rect)).text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                &node.text,
                egui::FontId::proportional((node.font_size * scale).clamp(3.0, 18.0)),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_persist_in_open_order_without_duplicates_and_refresh_edits() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Some(dir.path().join("recent.json"));
        let mut a = Document::blank().unwrap();
        let mut b = Document::blank().unwrap();
        a.save(&dir.path().join("a.mio")).unwrap();
        b.save(&dir.path().join("b.mio")).unwrap();
        let mut home = Home::default();
        home.observe_at(&a, storage.clone()).unwrap();
        home.observe_at(&b, storage.clone()).unwrap();
        home.observe_at(&a, storage.clone()).unwrap();
        assert_eq!(home.recent.len(), 2);
        assert_eq!(Some(&home.recent[0].path), a.path.as_ref());
        a.create_text_node([0.0, 0.0]).unwrap();
        home.observe_at(&a, storage.clone()).unwrap();
        assert_eq!(home.recent[0].preview.as_ref().unwrap().nodes.len(), 1);
        let mut restored = Home::default();
        restored
            .observe_at(&Document::blank().unwrap(), storage)
            .unwrap();
        assert_eq!(restored.recent.len(), 2);
        assert_eq!(Some(&restored.recent[0].path), a.path.as_ref());
    }

    #[test]
    fn invalid_recent_record_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recent.json");
        std::fs::write(&path, b"invalid").unwrap();
        assert!(
            Home::default()
                .observe_at(&Document::blank().unwrap(), Some(path))
                .unwrap_err()
                .contains("读取最近文件记录失败")
        );
    }

    #[test]
    fn removal_preserves_files_by_default_and_reports_disk_failures() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().join("recent.json");
        let mut doc = Document::blank().unwrap();
        doc.save(&dir.path().join("keep.mio")).unwrap();
        let path = doc.path.clone().unwrap();
        let mut home = Home::default();
        home.observe_at(&doc, Some(storage.clone())).unwrap();
        home.remove_recent_at(&path, false, Some(&storage)).unwrap();
        assert!(path.is_file());
        home.observe_at(&doc, Some(storage.clone())).unwrap();
        assert!(
            home.recent.is_empty(),
            "active document is not immediately reinserted"
        );
        let mut restored = Home::default();
        restored
            .observe_at(&Document::blank().unwrap(), Some(storage.clone()))
            .unwrap();
        assert!(restored.recent.is_empty());

        home.observed = None;
        home.observe_at(&doc, Some(storage.clone())).unwrap();
        home.remove_recent_at(&path, true, Some(&storage)).unwrap();
        assert!(!path.exists());
        assert!(home.recent.is_empty());

        doc = Document::blank().unwrap();
        doc.save(&path).unwrap();
        home.observed = None;
        home.observe_at(&doc, Some(storage.clone())).unwrap();
        let before = std::fs::read(&storage).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            home.remove_recent_at(&path, true, Some(&storage))
                .unwrap_err()
                .contains("删除本机文件失败")
        );
        assert_eq!(home.recent.len(), 1);
        assert_eq!(std::fs::read(&storage).unwrap(), before);
        assert!(
            home.remove_recent_at(&path, false, Some(dir.path()))
                .is_err()
        );
        assert_eq!(
            home.recent.len(),
            1,
            "failed persistence keeps the record retryable"
        );
    }

    #[test]
    fn location_uses_one_literal_directory_argument() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("中文 spaces ' $() ;");
        std::fs::create_dir(&folder).unwrap();
        let command = location_command(&folder.join("test.mio")).unwrap();
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [folder.canonicalize().unwrap().as_os_str()]
        );
        assert!(location_command(&dir.path().join("missing/file.mio")).is_err());
    }

    #[test]
    fn recent_card_context_menu_and_delete_dialog_preserve_the_file_until_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::blank().unwrap();
        doc.save(&dir.path().join("长中文名称 group-near.mio"))
            .unwrap();
        let path = doc.path.clone().unwrap();
        for light in [false, true] {
            for zoom in [1.0, 1.5, 2.0] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(zoom);
                let mut home = Home {
                    visible: true,
                    ..Default::default()
                };
                home.observe_at(&doc, None).unwrap();
                let run = |home: &mut Home, events| {
                    let mut action = None;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(900.0, 800.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |_| action = home.show(&ctx, light, |_| false),
                    );
                    output.textures_delta.clear();
                    (output, action)
                };
                for _ in 0..3 {
                    run(&mut home, vec![]);
                }
                let card = ctx
                    .read_response(egui::Id::new(("recent-file", &path)))
                    .unwrap()
                    .rect;
                let pos = card.center();
                run(
                    &mut home,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                    ],
                );
                run(
                    &mut home,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                );
                let (output, action) = run(&mut home, vec![]);
                assert!(action.is_none(), "right click must not open the document");
                let text_rect = |output: &egui::FullOutput, label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.job.text == label => {
                                Some(text.visual_bounding_rect())
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("missing UI label: {label}"))
                };
                text_rect(&output, "打开文件所在位置");
                let pos = text_rect(&output, "删除…").center();
                run(
                    &mut home,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                    ],
                );
                run(
                    &mut home,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                );
                let (output, action) = run(&mut home, vec![]);
                assert!(action.is_none());
                assert!(!home.deletion.as_ref().unwrap().delete_file);
                text_rect(&output, "同时删除本机文件（永久删除）");
                assert!(path.exists());
                let pos = text_rect(&output, "移除记录").center();
                run(
                    &mut home,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                    ],
                );
                let (_, action) = run(
                    &mut home,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                );
                assert!(matches!(
                    action,
                    Some(Action::RemoveRecent {
                        delete_file: false,
                        ..
                    })
                ));
                assert!(path.exists());
            }
        }
    }

    #[test]
    fn missing_recent_files_are_pruned_on_load_and_home_reentry() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().join("recent.json");
        let missing = dir.path().join("missing.mio");
        let corrupt = dir.path().join("corrupt.mio");
        std::fs::write(&corrupt, b"invalid document, but still a real file").unwrap();
        let mut doc = Document::blank().unwrap();
        doc.save(&dir.path().join("valid.mio")).unwrap();
        std::fs::write(
            &storage,
            serde_json::to_vec(&[&missing, &corrupt, doc.path.as_ref().unwrap()]).unwrap(),
        )
        .unwrap();
        let mut home = Home::default();
        home.observe_at(&doc, Some(storage.clone())).unwrap();
        assert_eq!(home.recent.len(), 2);
        std::fs::remove_file(doc.path.as_ref().unwrap()).unwrap();
        home.visible = true;
        home.observe_at(&doc, Some(storage.clone())).unwrap();
        assert_eq!(home.recent.len(), 1);
        assert_eq!(home.recent[0].path, corrupt);
        let saved: Vec<PathBuf> = serde_json::from_slice(&std::fs::read(storage).unwrap()).unwrap();
        assert_eq!(saved, [corrupt]);
        assert!(
            doc.path.is_some(),
            "cleanup does not close the in-memory document"
        );
    }
}

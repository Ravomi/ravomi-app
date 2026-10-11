use super::*;
use bevy::ecs::system::RunSystemOnce;
use bevy_egui::egui;
use project_graph_bevy_canvas::project;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

type OpenResult = std::result::Result<Option<Document>, String>;

struct Tab {
    id: u64,
    title: String,
    document: Option<Document>,
    view: Option<(Transform, Projection)>,
}

enum Action {
    Switch(usize),
    Open(Box<Document>),
    Close,
}

#[derive(Resource, Default)]
pub struct Tabs {
    entries: Vec<Tab>,
    active: usize,
    serial: u64,
    pending: Option<Action>,
    opening: Option<Mutex<mpsc::Receiver<OpenResult>>>,
    confirm_close: bool,
    hover: Hover,
    motion: HashMap<u64, TabMotion>,
    track_rect: Option<egui::Rect>,
    selections: HashMap<u64, HashSet<String>>,
    edge_selections: HashMap<u64, HashSet<String>>,
    pub(super) close_after_save: bool,
    exit_discarded: HashSet<u64>,
    exit_switch: bool,
    backups: Backups,
}

type BackupResult = std::result::Result<Vec<(u64, u64)>, String>;

#[derive(Default)]
struct Backups {
    policy: project_graph_bevy_canvas::recovery::Policy,
    loaded: bool,
    directory: Option<PathBuf>,
    last_attempt: Option<std::time::Instant>,
    session: Option<String>,
    completed: HashMap<u64, u64>,
    pending: Option<Mutex<mpsc::Receiver<BackupResult>>>,
}

pub fn backup(
    mut tabs: ResMut<Tabs>,
    mut scene: ResMut<Scene>,
    mut editor: ResMut<editor::Editor>,
    mut ai: Option<ResMut<canvas_ai::State>>,
) {
    if !tabs.backups.loaded {
        tabs.backups.loaded = true;
        match tabs
            .backup_directory()
            .and_then(|directory| project_graph_bevy_canvas::recovery::Policy::load(&directory))
        {
            Ok(policy) => tabs.backups.policy = policy,
            Err(error) => {
                tabs.backups.policy.enabled = false;
                editor.report_error(format!(
                    "无法读取备份设置，自动备份已暂停；请在恢复副本中重新保存设置：{error}"
                ));
            }
        }
    }
    if let Some(receiver) = &tabs.backups.pending {
        let result = match receiver.lock() {
            Ok(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err("后台备份任务意外结束".into())),
            },
            Err(_) => Some(Err("后台备份状态不可用".into())),
        };
        if let Some(result) = result {
            tabs.backups.pending = None;
            match result {
                Ok(revisions) => tabs.backups.completed.extend(revisions),
                Err(error) => editor.report_error(format!("自动恢复副本未保存：{error}")),
            }
        }
        return;
    }
    let now = std::time::Instant::now();
    let policy = tabs.backups.policy.clone();
    let last = tabs.backups.last_attempt.get_or_insert(now);
    if !policy.enabled
        || now.duration_since(*last) < Duration::from_secs(policy.interval_seconds)
        || editor.tabs_blocked()
    {
        return;
    }
    tabs.backups.last_attempt = Some(now);
    tabs.ensure(&scene.0);
    let snapshot = match ai
        .as_deref_mut()
        .map(canvas_ai::State::snapshot_for_save)
        .transpose()
    {
        Ok(snapshot) => snapshot,
        Err(error) => {
            editor.report_error(format!("无法准备协作恢复副本：{error}"));
            return;
        }
    };
    let mut captures = Vec::new();
    let tabs = &mut *tabs;
    for (index, tab) in tabs.entries.iter_mut().enumerate() {
        let document = if index == tabs.active {
            if editor.active.is_some() {
                continue;
            }
            &mut scene.0
        } else if let Some(document) = &mut tab.document {
            document
        } else {
            continue;
        };
        if document.in_transaction() {
            continue;
        }
        if let (Some(ai), Some(snapshot)) = (ai.as_deref(), snapshot.as_ref())
            && let Err(error) = ai.prepare_document(document, snapshot)
        {
            editor.report_error(format!("无法准备协作恢复副本：{error}"));
            return;
        }
        if !document.dirty() || tabs.backups.completed.get(&tab.id) == Some(&document.revision) {
            continue;
        }
        match document.capture_snapshot() {
            Ok(snapshot) => captures.push((tab.id, snapshot)),
            Err(error) => {
                editor.report_error(format!("无法准备恢复副本：{error}"));
                return;
            }
        }
    }
    if captures.is_empty() {
        return;
    }
    let root = match tabs
        .backups
        .directory
        .clone()
        .map(Ok)
        .unwrap_or_else(project_graph_bevy_canvas::recovery::root)
    {
        Ok(root) => root,
        Err(error) => {
            editor.report_error(error.to_string());
            return;
        }
    };
    let session = tabs
        .backups
        .session
        .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
        .clone();
    let (sender, receiver) = mpsc::channel();
    tabs.backups.pending = Some(Mutex::new(receiver));
    let policy = tabs.backups.policy.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<Vec<(u64, u64)>> {
            let mut completed = Vec::new();
            for (id, snapshot) in captures {
                project_graph_bevy_canvas::recovery::write_with_policy(
                    &snapshot,
                    &root.join(&session).join(id.to_string()),
                    &policy,
                )?;
                completed.push((id, snapshot.revision));
            }
            Ok(completed)
        })()
        .map_err(|error| error.to_string());
        // The application may close while this independent disk write finishes.
        let _ = sender.send(result);
    });
}

#[derive(Default)]
struct Hover {
    id: Option<u64>,
    left_at: Option<f64>,
}
impl Hover {
    fn update(&mut self, hovered: Option<u64>, now: f64) {
        if let Some(id) = hovered {
            self.id = Some(id);
            self.left_at = None;
        } else if self.id.is_some() {
            let left = *self.left_at.get_or_insert(now);
            if now - left >= 0.3 {
                self.id = None;
                self.left_at = None;
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Spring {
    pub value: f32,
    velocity: f32,
}
impl Spring {
    pub fn step(&mut self, target: f32, dt: f32) -> bool {
        // Closed-form damped oscillator retains velocity when a hover reverses.
        // A damping ratio of .82 gives a small rebound and settles in about 300 ms.
        let decay = 26.0 * 0.82;
        let frequency = 26.0 * (1.0_f32 - 0.82 * 0.82).sqrt();
        let displacement = self.value - target;
        let b = (self.velocity + decay * displacement) / frequency;
        let (sin, cos) = (frequency * dt).sin_cos();
        let envelope = (-decay * dt).exp();
        self.value = target + envelope * (displacement * cos + b * sin);
        self.velocity = envelope
            * ((b * frequency - decay * displacement) * cos
                - (displacement * frequency + decay * b) * sin);
        let moving = (self.value - target).abs() > 0.001 || self.velocity.abs() > 0.01;
        if !moving {
            self.value = target;
            self.velocity = 0.0;
        }
        moving
    }
}

#[derive(Default)]
struct TabMotion {
    expand: Spring,
    neighbor: Spring,
}

fn neighbor_level(index: usize, focal: usize) -> f32 {
    match index.abs_diff(focal) {
        1 => 1.0,
        2 => 0.45,
        _ => 0.0,
    }
}

fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    egui::Color32::from_rgb(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
    )
}

fn tab_label(painter: &egui::Painter, label: &str, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(
        label.into(),
        egui::FontId::proportional(16.0),
        egui::Color32::WHITE,
        width,
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    painter.layout_job(job)
}

fn title(doc: &Document) -> String {
    doc.path
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名画布".into())
}

impl Tabs {
    pub(super) fn document_mut<'a>(
        &'a mut self,
        active: &'a mut Document,
        token: uuid::Uuid,
    ) -> Option<&'a mut Document> {
        if active.instance_token() == token {
            Some(active)
        } else {
            self.entries
                .iter_mut()
                .filter_map(|tab| tab.document.as_mut())
                .find(|document| document.instance_token() == token)
        }
    }

    pub(super) fn instance_tokens(&self, active: &Document) -> HashSet<uuid::Uuid> {
        std::iter::once(active.instance_token())
            .chain(
                self.entries
                    .iter()
                    .filter_map(|tab| tab.document.as_ref())
                    .map(Document::instance_token),
            )
            .collect()
    }

    pub(super) fn for_each_document_mut(
        &mut self,
        active: &mut Document,
        mut update: impl FnMut(&mut Document) -> std::result::Result<(), String>,
    ) -> std::result::Result<(), String> {
        update(active)?;
        for document in self
            .entries
            .iter_mut()
            .filter_map(|tab| tab.document.as_mut())
        {
            update(document)?;
        }
        Ok(())
    }

    fn backup_directory(&self) -> Result<PathBuf> {
        self.backups
            .directory
            .clone()
            .map(Ok)
            .unwrap_or_else(project_graph_bevy_canvas::recovery::root)
    }

    pub fn backup_policy(&self) -> &project_graph_bevy_canvas::recovery::Policy {
        &self.backups.policy
    }

    pub fn configure_backups(
        &mut self,
        policy: project_graph_bevy_canvas::recovery::Policy,
    ) -> Result<()> {
        policy.save(&self.backup_directory()?)?;
        self.backups.policy = policy;
        self.backups.loaded = true;
        self.backups.last_attempt = Some(std::time::Instant::now());
        Ok(())
    }
    pub(super) fn closing_tab(&self) -> bool {
        self.confirm_close
    }

    pub(super) fn cancel_close(&mut self) {
        self.confirm_close = false;
        self.close_after_save = false;
    }

    pub(super) fn finish_close(&mut self) {
        self.cancel_close();
        self.pending = Some(Action::Close);
    }

    pub(super) fn cancel_exit(&mut self) {
        self.exit_discarded.clear();
        if self.exit_switch {
            self.pending = None;
        }
        self.exit_switch = false;
    }

    pub(super) fn discard_on_exit(&mut self) {
        self.exit_discarded.insert(self.entries[self.active].id);
    }

    pub(super) fn advance_exit(&mut self, doc: &Document) -> bool {
        self.ensure(doc);
        if doc.dirty() && !self.exit_discarded.contains(&self.entries[self.active].id) {
            return false;
        }
        if let Some(index) = self.entries.iter().position(|tab| {
            !self.exit_discarded.contains(&tab.id)
                && tab.document.as_ref().is_some_and(Document::dirty)
        }) {
            self.pending = Some(Action::Switch(index));
            self.exit_switch = true;
            false
        } else {
            true
        }
    }

    pub(super) fn path_in_other_tab(&self, path: &std::path::Path) -> bool {
        std::fs::canonicalize(path).ok().is_some_and(|path| {
            self.entries.iter().any(|t| {
                t.document
                    .as_ref()
                    .is_some_and(|doc| doc.path.as_ref() == Some(&path))
            })
        })
    }

    pub fn open_document(&mut self, document: Document) {
        self.pending = Some(Action::Open(Box::new(document)));
    }

    pub fn open_file(&mut self) {
        if self.opening.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .add_filter(
                    "Ravomi 项目与兼容画布",
                    &[project::EXTENSION, project::LEGACY_EXTENSION],
                )
                .pick_file()
                .map(|path| Document::open(&path).map_err(|e| e.to_string()))
                .transpose();
            let _ = tx.send(result);
        });
        self.opening = Some(Mutex::new(rx));
    }

    pub fn open_recovery(&mut self, path: PathBuf) {
        if self.opening.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = Document::open_recovery(&path)
                .map(Some)
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.opening = Some(Mutex::new(receiver));
    }

    pub fn from_args(initial: &Document, args: &[String]) -> Result<Self> {
        let mut tabs = Self::default();
        tabs.ensure(initial);
        for pair in args.windows(2).filter(|pair| pair[0] == "--tab") {
            let doc = Document::open(std::path::Path::new(&pair[1]))?;
            if doc.path == initial.path
                || tabs.entries.iter().any(|tab| {
                    tab.document
                        .as_ref()
                        .is_some_and(|open| open.path == doc.path)
                })
            {
                continue;
            }
            tabs.serial += 1;
            tabs.entries.push(Tab {
                id: tabs.serial,
                title: title(&doc),
                document: Some(doc),
                view: None,
            });
        }
        Ok(tabs)
    }

    pub fn modal(&self) -> bool {
        self.confirm_close || self.opening.is_some()
    }

    fn exchange(
        &mut self,
        doc: &mut Document,
        target: usize,
        view: Option<(Transform, Projection)>,
        closing: bool,
    ) -> Option<(Transform, Projection)> {
        let previous = self.active;
        self.entries[previous].view = view;
        let mut next = self.entries[target]
            .document
            .take()
            .expect("inactive tab document");
        next.revision = next.revision.max(doc.revision) + 1;
        self.entries[previous].document = Some(std::mem::replace(doc, next));
        self.active = target;
        let view = self.entries[target].view.clone();
        if closing {
            self.entries.remove(previous);
            if previous < target {
                self.active -= 1;
            }
        }
        view
    }
    pub fn dirty(&self) -> bool {
        self.entries
            .iter()
            .any(|t| t.document.as_ref().is_some_and(Document::dirty))
    }

    fn ensure(&mut self, doc: &Document) {
        if self.entries.is_empty() {
            self.entries.push(Tab {
                id: 0,
                title: title(doc),
                document: None,
                view: None,
            });
        }
        self.entries[self.active].title = title(doc);
    }

    pub fn bar(&mut self, ctx: &egui::Context, doc: &Document, editor: &mut editor::Editor) {
        self.ensure(doc);
        if let Some(receiver) = &self.opening {
            let result = receiver.lock().expect("open dialog receiver").try_recv();
            match result {
                Ok(result) => {
                    self.opening = None;
                    match result {
                        Ok(Some(doc)) => self.pending = Some(Action::Open(Box::new(doc))),
                        Ok(None) => {}
                        Err(error) => editor.report_error(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.opening = None;
                    editor.report_error("打开文件失败，请重试".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if editor.home_visible() {
            self.track_rect = None;
            return;
        }
        let enabled = !editor.tabs_blocked() && !editor.closing && !self.confirm_close;
        let (now, dt) = ctx.input(|i| (i.time, i.stable_dt.min(0.05)));
        let hovered = self
            .entries
            .iter()
            .find(|tab| {
                enabled
                    && ctx
                        .read_response(egui::Id::new(("tab", tab.id)))
                        .is_some_and(|r| r.hovered())
            })
            .map(|tab| tab.id);
        let over_track = enabled
            && ctx
                .pointer_hover_pos()
                .is_some_and(|p| self.track_rect.is_some_and(|r| r.contains(p)));
        if hovered.is_some() || !over_track {
            self.hover.update(hovered, now);
        } else {
            self.hover.left_at = None;
        }
        if self.hover.left_at.is_some() {
            ctx.request_repaint();
        }
        let focal = self
            .entries
            .iter()
            .position(|tab| Some(tab.id) == self.hover.id)
            .unwrap_or(self.active);
        self.motion
            .retain(|id, _| self.entries.iter().any(|tab| tab.id == *id));
        let palette = theme::dock_palette(editor.light);
        let border = palette.muted;
        let foreground = palette.active;
        let ink = palette.ink;
        let (left, right) = editor.dock_space.unwrap_or((
            ctx.content_rect().left(),
            ctx.content_rect().right() - 160.0,
        ));
        // Window controls reserve this space before drawing us. Shrink only the
        // document viewport when UI scaling reduces the available width.
        // Moving the whole capsule to a second row obscures the canvas.
        let width = (right - left - 32.0).clamp(104.0, 900.0);
        let documents_width = (width - 80.0).max(24.0);
        let area = egui::Area::new("document-tabs".into())
            .anchor(
                egui::Align2::CENTER_TOP,
                [
                    (left + right) * 0.5 - ctx.content_rect().center().x,
                    theme::dock_top(ctx),
                ],
            )
            .show(ctx, |ui| {
                ui.set_max_width(width);
                theme::dock_frame(editor.light).show(ui, |ui| {
                    theme::dock_style(ui, editor.light);
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            // Keep actions outside the scrolling viewport; egui's rectangular
                            // overflow fades otherwise cover the capsule and its close button.
                            ui.spacing_mut().scroll.fade.strength = 0.0;
                            egui::ScrollArea::horizontal()
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                )
                                .min_scrolled_width(24.0)
                                .max_width(documents_width)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        for (i, tab) in self.entries.iter().enumerate() {
                                            let id = egui::Id::new(("tab", tab.id));
                                            let selected = self.active == i;
                                            let motion =
                                                self.motion.entry(tab.id).or_insert_with(|| {
                                                    TabMotion {
                                                        expand: Spring {
                                                            value: if selected { 1.0 } else { 0.0 },
                                                            ..default()
                                                        },
                                                        ..default()
                                                    }
                                                });
                                            let expanding = motion.expand.step(
                                                if selected || self.hover.id == Some(tab.id) {
                                                    1.0
                                                } else {
                                                    0.0
                                                },
                                                dt,
                                            );
                                            let neighboring =
                                                motion.neighbor.step(neighbor_level(i, focal), dt);
                                            if expanding || neighboring {
                                                ctx.request_repaint();
                                            }
                                            let expanded = motion.expand.value.clamp(0.0, 1.0);
                                            let neighbor = motion.neighbor.value.clamp(0.0, 1.05);
                                            let label = format!(
                                                "{}{}",
                                                tab.title,
                                                if tab.document.as_ref().unwrap_or(doc).dirty() {
                                                    " •"
                                                } else {
                                                    ""
                                                }
                                            );
                                            let text = ui.painter().layout_no_wrap(
                                                label.clone(),
                                                egui::FontId::proportional(16.0),
                                                egui::Color32::WHITE,
                                            );
                                            let expanded_width = (text.size().x + 24.0)
                                                .clamp(88.0, 240.0)
                                                .min(documents_width);
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(
                                                    20.0 + (expanded_width - 20.0) * expanded,
                                                    40.0,
                                                ),
                                                egui::Sense::hover(),
                                            );
                                            let response =
                                                ui.interact(rect, id, egui::Sense::click());
                                            let stem = 6.0 + 8.0 * neighbor;
                                            let bar = egui::Rect::from_center_size(
                                                rect.center(),
                                                egui::vec2(
                                                    stem + (expanded_width - stem) * expanded,
                                                    20.0 + 8.0 * neighbor * (1.0 - expanded)
                                                        + 12.0 * expanded,
                                                ),
                                            );
                                            ui.painter().rect_filled(
                                                bar,
                                                14.0,
                                                mix(
                                                    mix(border, foreground, neighbor * 0.6),
                                                    foreground,
                                                    expanded,
                                                ),
                                            );
                                            let opacity =
                                                ((expanded - 0.35) / 0.65).clamp(0.0, 1.0);
                                            if opacity > 0.0 {
                                                let text = tab_label(
                                                    ui.painter(),
                                                    &label,
                                                    (expanded_width - 24.0).max(1.0),
                                                );
                                                let metrics = ui.painter().layout_no_wrap(
                                                    "国Hgyp".into(),
                                                    egui::FontId::proportional(16.0),
                                                    egui::Color32::WHITE,
                                                );
                                                let text_position = egui::pos2(
                                                    bar.left() + 12.0,
                                                    // Share a baseline across titles, including
                                                    // CJK, Latin descenders and the unsaved dot.
                                                    bar.center().y - metrics.mesh_bounds.center().y,
                                                );
                                                let painter = ui.painter().with_clip_rect(
                                                    bar.shrink2(egui::vec2(6.0, 2.0)),
                                                );
                                                let blur = 5.0 * (1.0 - opacity);
                                                let taps = if blur > 0.1 { 9 } else { 1 };
                                                // A small Gaussian text kernel approximates Framer's blur without a full-screen render pass.
                                                for tap in 0..taps {
                                                    let (offset, weight) = if taps == 1 {
                                                        (egui::Vec2::ZERO, 1.0)
                                                    } else if tap == 0 {
                                                        (egui::Vec2::ZERO, 0.4)
                                                    } else {
                                                        let angle = (tap - 1) as f32
                                                            * std::f32::consts::TAU
                                                            / 8.0;
                                                        (
                                                            egui::vec2(angle.cos(), angle.sin())
                                                                * blur,
                                                            0.075,
                                                        )
                                                    };
                                                    painter.galley_with_override_text_color(
                                                        text_position + offset,
                                                        text.clone(),
                                                        ink.gamma_multiply(opacity * weight),
                                                    );
                                                }
                                            }
                                            response.widget_info(|| {
                                                egui::WidgetInfo::selected(
                                                    egui::WidgetType::SelectableLabel,
                                                    enabled,
                                                    selected,
                                                    &label,
                                                )
                                            });
                                            if response.clicked() {
                                                self.pending = Some(Action::Switch(i));
                                            }
                                            response.on_hover_text(&label);
                                        }
                                    });
                                });
                            if ui
                                .add_enabled(
                                    self.opening.is_none(),
                                    egui::Button::new("＋")
                                        .frame(false)
                                        .min_size(egui::vec2(32.0, 40.0)),
                                )
                                .on_hover_text("打开文件到新标签页")
                                .clicked()
                            {
                                self.open_file();
                            }
                            if theme::close_button(ui, editor.light, "关闭当前标签页").clicked()
                            {
                                if doc.dirty() || editor.active.is_some() {
                                    self.confirm_close = true;
                                } else {
                                    self.pending = Some(Action::Close);
                                }
                            }
                        });
                    });
                });
            });
        self.track_rect = Some(area.response.rect);
        if self.opening.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

pub fn apply(world: &mut World) {
    if world.resource::<editor::Editor>().tabs_blocked()
        || (world.resource::<editor::Editor>().closing && !world.resource::<Tabs>().exit_switch)
    {
        return;
    }
    let Some(action) = world.resource_mut::<Tabs>().pending.take() else {
        return;
    };
    world.resource_mut::<Tabs>().exit_switch = false;
    world.resource_mut::<editor::Editor>().hide_home();
    world
        .resource_mut::<editor::Editor>()
        .tools
        .reset_for_document();
    let mut tabs = world.remove_resource::<Tabs>().expect("tabs resource");
    let mut scene = world.remove_resource::<Scene>().expect("scene resource");
    tabs.ensure(&scene.0);
    let closing = matches!(action, Action::Close);
    let target = match action {
        Action::Switch(index) => index,
        Action::Close => {
            if tabs.entries.len() > 1 {
                if tabs.active == 0 { 1 } else { tabs.active - 1 }
            } else {
                match Document::blank() {
                    Ok(doc) => {
                        tabs.serial += 1;
                        tabs.entries.push(Tab {
                            id: tabs.serial,
                            title: title(&doc),
                            document: Some(doc),
                            view: None,
                        });
                        1
                    }
                    Err(error) => {
                        world
                            .resource_mut::<editor::Editor>()
                            .report_error(error.to_string());
                        world.insert_resource(scene);
                        world.insert_resource(tabs);
                        return;
                    }
                }
            }
        }
        Action::Open(doc) => {
            let existing = if doc.path.is_some() && doc.path == scene.0.path {
                Some(tabs.active)
            } else {
                tabs.entries.iter().position(|t| {
                    t.document
                        .as_ref()
                        .is_some_and(|d| doc.path.is_some() && d.path == doc.path)
                })
            };
            existing.unwrap_or_else(|| {
                tabs.serial += 1;
                tabs.entries.push(Tab {
                    id: tabs.serial,
                    title: title(&doc),
                    document: Some(*doc),
                    view: None,
                });
                tabs.entries.len() - 1
            })
        }
    };
    if target == tabs.active || target >= tabs.entries.len() {
        world.insert_resource(scene);
        world.insert_resource(tabs);
        return;
    }
    world
        .resource_mut::<physics::PhysicsEditor>()
        .finish(&mut scene.0);
    if !world.resource_mut::<editor::Editor>().commit(&mut scene.0) {
        world.insert_resource(scene);
        world.insert_resource(tabs);
        return;
    }
    let current_view = world
        .query_filtered::<(&Transform, &Projection), With<Camera2d>>()
        .single(world)
        .ok()
        .map(|(t, p)| (*t, p.clone()));
    let previous_id = tabs.entries[tabs.active].id;
    let selected = world
        .get_resource::<Interaction>()
        .map(|interaction| {
            interaction
                .selected
                .iter()
                .filter_map(|&i| scene.0.nodes.get(i))
                .map(|n| n.id.clone())
                .collect()
        })
        .unwrap_or_default();
    tabs.selections.insert(previous_id, selected);
    tabs.edge_selections.insert(
        previous_id,
        world
            .get_resource::<Interaction>()
            .map(|i| i.selected_edges.clone())
            .unwrap_or_default(),
    );
    let view = tabs.exchange(&mut scene.0, target, current_view, closing);
    if let Some(mut effects) = world.get_resource_mut::<super::effects::Effects>() {
        effects.clear();
    }
    if closing {
        tabs.selections.remove(&previous_id);
        tabs.edge_selections.remove(&previous_id);
    }
    let selected = tabs
        .selections
        .get(&tabs.entries[tabs.active].id)
        .map(|ids| {
            scene
                .0
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| ids.contains(&n.id))
                .map(|(i, _)| i)
                .collect()
        })
        .unwrap_or_default();
    let entities: Vec<_> = world
        .query_filtered::<Entity, Or<(
            With<NodeVisual>,
            With<edges::EdgeRoot>,
            With<physics::BodyNode>,
        )>>()
        .iter(world)
        .collect();
    for entity in entities {
        world.despawn(entity);
    }
    let enabled = world.resource::<physics::PhysicsEditor>().enabled;
    world.insert_resource(physics::PhysicsEditor::default());
    world.resource_mut::<physics::PhysicsEditor>().enabled = enabled;
    world.insert_resource(Interaction {
        dirty: true,
        selected,
        selected_edges: tabs
            .edge_selections
            .get(&tabs.entries[tabs.active].id)
            .cloned()
            .unwrap_or_default(),
        selection_revision: scene.0.revision,
        ..default()
    });
    world.insert_resource(navigation::WheelZoom::default());
    world.insert_resource(effects::Effects::default());
    world.insert_resource(edges::EdgePaths::default());
    if let Some(mut display) = world.get_resource_mut::<sections::Display>() {
        *display = sections::Display::new(display.enabled);
    }
    world.insert_resource(scene);
    world.insert_resource(tabs);
    world.resource_mut::<Config>().edit_node = None;
    if let Err(error) = world.run_system_once(setup) {
        world
            .resource_mut::<editor::Editor>()
            .report_error(error.to_string());
    }
    if let Some((transform, projection)) = view
        && let Ok((mut t, mut p)) = world
            .query_filtered::<(&mut Transform, &mut Projection), With<Camera2d>>()
            .single_mut(world)
    {
        *t = transform;
        *p = projection;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_policy_disables_scheduling_and_reenabling_keeps_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let mut tabs = Tabs::default();
        tabs.backups.directory = Some(directory.path().to_owned());
        let policy = project_graph_bevy_canvas::recovery::Policy {
            enabled: false,
            interval_seconds: 15,
            generations: 3,
        };
        tabs.configure_backups(policy.clone()).unwrap();
        tabs.backups.last_attempt = Some(std::time::Instant::now() - Duration::from_secs(61));
        let mut world = World::new();
        world.insert_resource(tabs);
        world.insert_resource(Scene(doc));
        world.insert_resource(editor::Editor::default());
        world.run_system_once(backup).unwrap();
        assert!(world.resource::<Tabs>().backups.pending.is_none());
        let mut enabled = policy;
        enabled.enabled = true;
        world
            .resource_mut::<Tabs>()
            .configure_backups(enabled.clone())
            .unwrap();
        assert_eq!(
            project_graph_bevy_canvas::recovery::Policy::load(directory.path()).unwrap(),
            enabled
        );
        world.run_system_once(backup).unwrap();
        assert!(world.resource::<Tabs>().backups.pending.is_none());
        let old = directory.path().join("keep.txt");
        std::fs::write(&old, b"existing").unwrap();
        let mut invalid = enabled.clone();
        invalid.generations = 0;
        assert!(
            world
                .resource_mut::<Tabs>()
                .configure_backups(invalid)
                .is_err()
        );
        assert_eq!(world.resource::<Tabs>().backup_policy(), &enabled);
        assert_eq!(std::fs::read(old).unwrap(), b"existing");
    }

    #[test]
    fn automatic_backup_captures_active_and_inactive_tabs_without_marking_saved() {
        let directory = tempfile::tempdir().unwrap();
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let mut other = Document::blank().unwrap();
        other.create_text_node([100.0; 2]).unwrap();
        let mut tabs = Tabs::default();
        tabs.ensure(&doc);
        tabs.entries.push(Tab {
            id: 99,
            title: "other".into(),
            document: Some(other),
            view: None,
        });
        tabs.backups.directory = Some(directory.path().to_owned());
        tabs.backups.last_attempt = Some(std::time::Instant::now() - Duration::from_secs(61));
        let mut world = World::new();
        world.insert_resource(tabs);
        world.insert_resource(Scene(doc));
        world.insert_resource(editor::Editor::default());
        world.run_system_once(backup).unwrap();
        assert!(world.resource::<Tabs>().backups.pending.is_some());
        world
            .resource_mut::<Scene>()
            .0
            .create_text_node([500.0; 2])
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while world.resource::<Tabs>().backups.pending.is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
            world.run_system_once(backup).unwrap();
        }
        let tabs = world.resource::<Tabs>();
        assert_eq!(tabs.backups.completed.len(), 2);
        let session = tabs.backups.session.as_ref().unwrap();
        for tab in &tabs.entries {
            let entries = project_graph_bevy_canvas::recovery::list(
                &directory.path().join(session).join(tab.id.to_string()),
            )
            .unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(
                Document::open(
                    &project_graph_bevy_canvas::recovery::document_path(&entries[0].0).unwrap()
                )
                .unwrap()
                .nodes
                .len(),
                1
            );
        }
        assert!(world.resource::<Scene>().0.dirty());
        assert_eq!(world.resource::<Scene>().0.nodes.len(), 2);
        assert!(tabs.entries[1].document.as_ref().unwrap().dirty());
    }

    #[test]
    fn exit_visits_each_dirty_document_and_cancel_keeps_all_documents() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let mut other = Document::blank().unwrap();
        other.create_text_node([100.0; 2]).unwrap();
        let mut tabs = Tabs::default();
        tabs.ensure(&doc);
        tabs.entries.push(Tab {
            id: 1,
            title: "B".into(),
            document: Some(other),
            view: None,
        });
        assert!(!tabs.advance_exit(&doc));
        assert!(tabs.pending.is_none());
        tabs.discard_on_exit();
        assert!(!tabs.advance_exit(&doc));
        assert!(matches!(tabs.pending, Some(Action::Switch(1))));
        tabs.cancel_exit();
        assert!(tabs.pending.is_none());
        assert!(tabs.exit_discarded.is_empty());
        assert!(doc.dirty());
        assert!(tabs.entries[1].document.as_ref().unwrap().dirty());
        tabs.discard_on_exit();
        tabs.exchange(&mut doc, 1, None, false);
        assert!(!tabs.advance_exit(&doc));
        tabs.discard_on_exit();
        assert!(tabs.advance_exit(&doc));
        assert_eq!(tabs.entries.len(), 2);
    }

    #[test]
    fn save_as_rejects_another_open_document_path_and_cancel_close_is_non_destructive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.mio");
        let mut other = Document::blank().unwrap();
        other.save(&path).unwrap();
        let doc = Document::blank().unwrap();
        let mut tabs = Tabs::default();
        tabs.ensure(&doc);
        tabs.entries.push(Tab {
            id: 1,
            title: "B".into(),
            document: Some(other),
            view: None,
        });
        assert!(tabs.path_in_other_tab(&dir.path().join("./other.mio")));
        assert!(tabs.path_in_other_tab(&project::save_path(&dir.path().join("other")).unwrap()));
        assert!(!tabs.path_in_other_tab(&project::save_path(&dir.path().join("new")).unwrap()));
        tabs.confirm_close = true;
        tabs.close_after_save = true;
        tabs.cancel_close();
        assert!(!tabs.closing_tab());
        assert!(!tabs.close_after_save);
        assert!(tabs.pending.is_none());
        assert_eq!(tabs.entries.len(), 2);
    }

    #[test]
    fn switching_retains_dirty_history_and_view_and_closing_reindexes() {
        let mut doc = stress::synthetic(1, false, false, false).unwrap();
        let id = doc.nodes[0].id.clone();
        let original = doc.nodes[0].text.clone();
        doc.edit_text(&id, "未保存的文字".into(), [100.0, 50.0]);
        let mut tabs = Tabs::default();
        tabs.ensure(&doc);
        tabs.entries.push(Tab {
            id: 1,
            title: "B".into(),
            document: Some(stress::synthetic(2, false, false, false).unwrap()),
            view: None,
        });
        let view = (
            Transform::from_xyz(123.0, -456.0, 0.0),
            Projection::Orthographic(OrthographicProjection {
                scale: 2.5,
                ..OrthographicProjection::default_2d()
            }),
        );
        assert!(tabs.exchange(&mut doc, 1, Some(view), false).is_none());
        assert_eq!(doc.nodes.len(), 2);
        assert!(tabs.dirty());
        let restored = tabs.exchange(&mut doc, 0, None, true).unwrap();
        assert_eq!(tabs.active, 0);
        assert_eq!(tabs.entries.len(), 1);
        assert_eq!(restored.0.translation.x, 123.0);
        assert!(matches!(restored.1, Projection::Orthographic(p) if p.scale == 2.5));
        assert_eq!(doc.nodes[0].text, "未保存的文字");
        assert!(doc.dirty());
        doc.undo();
        assert_eq!(doc.nodes[0].text, original);
    }

    #[test]
    fn closing_first_tab_keeps_the_next_document_active() {
        let mut doc = stress::synthetic(1, false, false, false).unwrap();
        let mut tabs = Tabs::default();
        tabs.ensure(&doc);
        tabs.entries.push(Tab {
            id: 1,
            title: "B".into(),
            document: Some(stress::synthetic(2, false, false, false).unwrap()),
            view: None,
        });
        tabs.exchange(&mut doc, 1, None, true);
        assert_eq!(tabs.active, 0);
        assert!(tabs.entries[0].document.is_none());
        assert_eq!(doc.nodes.len(), 2);
    }

    #[test]
    fn spring_rebounds_settles_and_preserves_velocity_on_reversal() {
        for fps in [30, 60, 120] {
            let mut spring = Spring::default();
            let mut peak = 0.0_f32;
            for _ in 0..fps {
                spring.step(1.0, 1.0 / fps as f32);
                peak = peak.max(spring.value);
            }
            assert!(peak > 1.0 && peak < 1.02);
            assert_eq!(spring.value, 1.0);
            assert!(!spring.step(1.0, 1.0 / fps as f32));
        }
        let mut spring = Spring::default();
        spring.step(1.0, 0.05);
        let before = spring.value;
        let velocity = spring.velocity;
        spring.step(0.0, 0.0);
        assert!((spring.value - before).abs() < 1e-6);
        assert!((spring.velocity - velocity).abs() < 1e-5);
        for _ in 0..60 {
            spring.step(0.0, 1.0 / 60.0);
        }
        assert_eq!(spring.value, 0.0);
    }

    #[test]
    fn leaving_delays_collapse_and_reentry_cancels_it() {
        let mut hover = Hover::default();
        hover.update(Some(2), 0.0);
        hover.update(None, 0.1);
        hover.update(None, 0.39);
        assert_eq!(hover.id, Some(2));
        hover.update(Some(3), 0.4);
        assert_eq!(hover.id, Some(3));
        hover.update(None, 0.5);
        hover.update(None, 0.81);
        assert_eq!(hover.id, None);
        assert_eq!(neighbor_level(2, 3), 1.0);
        assert_eq!(neighbor_level(1, 3), 0.45);
        assert_eq!(neighbor_level(0, 3), 0.0);
    }

    #[test]
    fn themes_use_opposite_highlight_and_ink_polarities() {
        let dark = theme::palette(false);
        let light = theme::palette(true);
        assert!(theme::egui_color(dark.text).r() > theme::egui_color(dark.canvas).r());
        assert!(theme::egui_color(light.text).r() < theme::egui_color(light.canvas).r());
        assert_ne!(
            theme::egui_color(dark.surface),
            theme::egui_color(light.surface)
        );
        for p in [dark, light] {
            let text = theme::egui_color(p.text);
            assert_eq!(mix(theme::egui_color(p.border), text, 1.02), text);
        }
    }

    #[test]
    fn applying_a_switch_replaces_visuals_and_reuses_the_camera() {
        let mut world = World::new();
        let doc = stress::synthetic(1, false, false, false).unwrap();
        let mut tabs = Tabs::default();
        tabs.ensure(&doc);
        tabs.entries.push(Tab {
            id: 1,
            title: "B".into(),
            document: Some(stress::synthetic(2, false, false, false).unwrap()),
            view: None,
        });
        tabs.pending = Some(Action::Switch(1));
        world.insert_resource(tabs);
        world.insert_resource(Scene(doc));
        world.insert_resource(Interaction {
            selected: HashSet::from([0]),
            ..default()
        });
        let ctx = egui::Context::default();
        let icon = ctx.load_texture(
            "context-menu-icon",
            egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]),
            default(),
        );
        let icon_id = icon.id();
        let mut editor = editor::Editor::default();
        editor.tools.icons.push(icon);
        editor.tools.context = Some((egui::pos2(20.0, 20.0), Vec2::ZERO));
        world.insert_resource(editor);
        world.insert_resource(physics::PhysicsEditor::default());
        world.insert_resource(Assets::<Font>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(Assets::<edges::EdgeMaterial>::default());
        world.insert_resource(Config {
            font: vec![],
            benchmark: false,
            output: PathBuf::new(),
            screenshot: None,
            light: false,
            ui_scale: 1.0,
            physics: false,
            no_ui: false,
            preview: false,
            edit_node: None,
            diagnostics: false,
        });
        world.spawn((Window::default(), PrimaryWindow));
        let camera = world
            .spawn((Camera2d, Transform::from_xyz(120.0, 50.0, 0.0)))
            .id();
        let old = world.spawn((NodeVisual(0),)).id();
        let mut display = sections::Display::new(true);
        display.graph = Some(stress::synthetic(4, false, false, false).unwrap().graph);
        display.visible = vec![false; 4];
        world.insert_resource(display);
        let mut paths = edges::EdgePaths::default();
        paths.0.insert(
            "old-tab-edge".into(),
            edges::EdgePath {
                anchor: Vec2::ZERO,
                segments: vec![[Vec2::ZERO, Vec2::ONE]],
            },
        );
        world.insert_resource(paths);
        apply(&mut world);
        let tools = &world.resource::<editor::Editor>().tools;
        assert!(
            !tools.visible(),
            "tab switches close the previous document's tools"
        );
        assert_eq!(
            tools.icons.first().map(egui::TextureHandle::id),
            Some(icon_id)
        );
        assert!(world.resource::<sections::Display>().enabled);
        assert!(world.resource::<sections::Display>().graph.is_none());
        assert!(world.resource::<sections::Display>().visible.is_empty());
        assert!(world.resource::<edges::EdgePaths>().0.is_empty());
        assert!(world.get_entity(old).is_err());
        assert_eq!(
            world
                .query_filtered::<Entity, With<Camera2d>>()
                .iter(&world)
                .collect::<Vec<_>>(),
            [camera]
        );
        assert_eq!(world.query::<&NodeVisual>().iter(&world).count(), 2);
        assert_eq!(world.resource::<Scene>().0.nodes.len(), 2);
        let font = world.resource::<NodeAssets>().font.clone();
        let font_count = world.resource::<Assets<Font>>().len();
        assert!(world.resource::<Interaction>().selected.is_empty());
        world.resource_mut::<Interaction>().selected.insert(1);
        world.resource_mut::<Tabs>().pending = Some(Action::Switch(0));
        apply(&mut world);
        assert_eq!(world.query::<&NodeVisual>().iter(&world).count(), 1);
        assert_eq!(world.get::<Transform>(camera).unwrap().translation.x, 120.0);
        assert_eq!(world.resource::<Interaction>().selected, HashSet::from([0]));
        world.resource_mut::<Tabs>().pending = Some(Action::Switch(1));
        apply(&mut world);
        assert_eq!(world.resource::<Interaction>().selected, HashSet::from([1]));
        assert_eq!(world.resource::<NodeAssets>().font, font);
        assert_eq!(world.resource::<Assets<Font>>().len(), font_count);
        for _ in 0..2 {
            world.resource_mut::<Tabs>().finish_close();
            apply(&mut world);
        }
        assert_eq!(world.resource::<Tabs>().entries.len(), 1);
        assert!(world.resource::<Scene>().0.nodes.is_empty());
        assert!(!world.resource::<Scene>().0.dirty());
    }

    #[test]
    fn settled_document_label_fits_its_actual_clip_rect() {
        check_document_label_clip(egui::FontDefinitions::default());
    }

    #[test]
    #[ignore = "requires the installed CJK UI font"]
    fn cjk_document_label_descenders_fit_the_clip_rect() {
        let path = std::env::var("GRAPLU_TEST_FONT").unwrap_or_else(|_| {
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc".into()
        });
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "CJK".into(),
            egui::FontData::from_owned(std::fs::read(path).expect("installed CJK font")).into(),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "CJK".into());
        check_document_label_clip(fonts);
    }

    fn check_document_label_clip(fonts: egui::FontDefinitions) {
        let mut doc = Document::blank().unwrap();
        let mut baselines = HashMap::new();
        for label in [
            "group-near",
            "jumpy-gqp",
            "分组ÉÅgy",
            "tutorial-main-3.2",
            "group-near •",
        ] {
            doc.path = Some(format!("{label}.prg").into());
            for pixels_per_point in [1.0, 1.75, 2.625, 3.5] {
                for gap in [180.0, 240.0, 300.0, 400.0] {
                    let ctx = egui::Context::default();
                    ctx.set_fonts(fonts.clone());
                    ctx.set_pixels_per_point(pixels_per_point);
                    let mut tabs = Tabs::default();
                    let mut editor = editor::Editor::default();
                    editor.dock_space = Some((100.0, 100.0 + gap));
                    for frame in 0..4 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(800.0, 500.0),
                                )),
                                time: Some(frame as f64 / 60.0),
                                ..default()
                            },
                            |_| tabs.bar(&ctx, &doc, &mut editor),
                        );
                        if frame == 3 {
                            let labels = output
                                .shapes
                                .iter()
                                .filter_map(|shape| {
                                    if let egui::Shape::Text(text) = &shape.shape
                                        && text.galley.job.text.starts_with(label)
                                    {
                                        Some((shape.clip_rect, text))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();
                            assert!(!labels.is_empty());
                            for (clip, text) in labels {
                                let baseline = baselines
                                    .entry((pixels_per_point.to_bits(), gap.to_bits()))
                                    .or_insert(text.pos.y);
                                assert!(
                                    (text.pos.y - *baseline).abs() < 0.01,
                                    "title-dependent baseline: {label}"
                                );
                                let bounds = text.visual_bounding_rect();
                                assert!(
                                    clip.expand(0.5).contains_rect(bounds),
                                    "DPI {pixels_per_point}, gap {gap}: {bounds:?} clipped by {clip:?}"
                                );
                            }
                        }
                        output.textures_delta.clear();
                    }
                }
            }
        }
    }

    #[test]
    fn constrained_tab_bar_stays_in_the_reserved_top_row() {
        let doc = stress::synthetic(1, false, false, false).unwrap();
        for light in [false, true] {
            for (left, gap) in [
                (0.0, 144.0),
                (0.0, 240.0),
                (0.0, 600.0),
                (240.0, 144.0),
                (240.0, 180.0),
                (240.0, 240.0),
                (240.0, 300.0),
            ] {
                let ctx = egui::Context::default();
                let mut tabs = Tabs::default();
                let mut editor = editor::Editor::default();
                editor.light = light;
                editor.dock_space = Some((left, left + gap));
                for frame in 0..4 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(800.0, 500.0),
                            )),
                            time: Some(frame as f64 / 60.0),
                            ..default()
                        },
                        |_| tabs.bar(&ctx, &doc, &mut editor),
                    );
                    output.textures_delta.clear();
                }
                let rect = tabs.track_rect.unwrap();
                assert!((rect.top() - theme::dock_top(&ctx)).abs() < 1.0, "{rect:?}");
                assert!(
                    rect.left() >= left && rect.right() <= left + gap,
                    "{rect:?}"
                );
                assert!(rect.height() < 60.0);
            }
        }
    }

    #[test]
    fn hover_expands_without_switching_and_active_tab_stays_expanded() {
        let doc = stress::synthetic(1, false, false, false).unwrap();
        let mut tabs = Tabs::default();
        tabs.ensure(&doc);
        tabs.entries.push(Tab {
            id: 1,
            title: "Second".into(),
            document: Some(stress::synthetic(2, false, false, false).unwrap()),
            view: None,
        });
        let ctx = egui::Context::default();
        let mut editor = editor::Editor::default();
        let second = egui::Id::new(("tab", 1_u64));
        for frame in 0..5 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(frame as f64 * 0.1),
                    ..default()
                },
                |_| tabs.bar(&ctx, &doc, &mut editor),
            );
            output.textures_delta.clear();
        }
        let collapsed = ctx.read_response(second).unwrap().rect.width();
        for frame in 5..20 {
            let point = ctx.read_response(second).unwrap().rect.center();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(frame as f64 * 0.1),
                    events: vec![egui::Event::PointerMoved(point)],
                    ..default()
                },
                |_| tabs.bar(&ctx, &doc, &mut editor),
            );
            output.textures_delta.clear();
        }
        assert!(ctx.read_response(second).unwrap().rect.width() > collapsed + 40.0);
        assert_eq!(tabs.active, 0);
        assert!(tabs.pending.is_none());
        for frame in 20..65 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(frame as f64 * 0.1),
                    events: vec![egui::Event::PointerGone],
                    ..default()
                },
                |_| tabs.bar(&ctx, &doc, &mut editor),
            );
            output.textures_delta.clear();
        }
        assert!(
            ctx.read_response(egui::Id::new(("tab", 0_u64)))
                .unwrap()
                .rect
                .width()
                >= 100.0
        );
        assert!(ctx.read_response(second).unwrap().rect.width() <= collapsed + 1.0);
    }
}

use super::theme;
use bevy_egui::egui;
use project_graph_bevy_canvas::recovery::{self, Catalog, Policy};
use std::{
    path::PathBuf,
    sync::{Mutex, mpsc},
};

type ScanResult = Result<Catalog, String>;

pub enum Action {
    Restore(PathBuf),
    Configure(Policy),
}

#[derive(Default)]
pub struct Recovery {
    pub open: bool,
    catalog: Catalog,
    pending: Option<Mutex<mpsc::Receiver<ScanResult>>>,
    pub(super) settings: bool,
    draft: Option<Policy>,
    confirm_delete: Option<PathBuf>,
}

impl Recovery {
    pub fn refresh(&mut self) {
        self.scan(None);
    }

    fn scan(&mut self, delete: Option<PathBuf>) {
        self.open = true;
        if self.pending.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(Mutex::new(receiver));
        self.confirm_delete = None;
        std::thread::spawn(move || {
            let result = recovery::root()
                .and_then(|root| {
                    if let Some(path) = delete {
                        recovery::remove(&root, &path)?;
                    }
                    recovery::discover(&root)
                })
                .map_err(|error| error.to_string());
            // The window may close before the directory scan finishes.
            let _ = sender.send(result);
        });
    }

    pub fn show(&mut self, ctx: &egui::Context, light: bool, policy: &Policy) -> Option<Action> {
        if !self.open {
            return None;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.open = false;
            return None;
        }
        if let Some(receiver) = &self.pending {
            let result = match receiver.lock() {
                Ok(receiver) => match receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("恢复列表读取任务意外结束".into()))
                    }
                },
                Err(_) => Some(Err("恢复列表状态不可用".into())),
            };
            if let Some(result) = result {
                self.pending = None;
                match result {
                    Ok(catalog) => self.catalog = catalog,
                    Err(error) => self.catalog.errors = vec![error],
                }
            }
        }
        let mut chosen = None;
        let mut refresh = false;
        let mut delete = None;
        let mut restore_error = None;
        egui::Modal::new("recovery-browser".into())
            .frame(theme::tool_frame(light))
            .show(ctx, |ui| {
                theme::tool_style(ui, light);
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                ui.set_height((ctx.content_rect().height() - 96.0).clamp(220.0, 500.0));
                ui.set_width(480.0_f32.min(ctx.content_rect().width() - 64.0).max(200.0));
                self.open = !theme::tool_header(ui, if self.settings { "备份设置" } else { "恢复副本" }, light);
                ui.label(if self.settings { "自动保存独立的恢复副本，原文件仍需手动保存。" } else { "恢复会打开新草稿。检查内容后另存为，原文件不会被替换。" });
                if !self.settings {
                    refresh = ui.add_enabled(self.pending.is_none(), egui::Button::new("刷新列表")).clicked();
                }
                if ui.button(if self.settings { "返回恢复列表" } else { "备份设置" }).clicked() {
                    self.settings = !self.settings;
                    self.draft = Some(policy.clone());
                }
                if self.pending.is_some() {
                    ui.label("正在处理恢复副本…");
                }
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.settings {
                            let draft = self.draft.get_or_insert_with(|| policy.clone());
                            ui.checkbox(&mut draft.enabled, "自动保存恢复副本");
                            egui::Grid::new("backup-policy-fields").show(ui, |ui| {
                                ui.label("备份间隔（秒）");
                                ui.add(egui::DragValue::new(&mut draft.interval_seconds).range(15..=3600));
                                ui.end_row();
                                ui.label("每个会话标签保留（份）");
                                ui.add(egui::DragValue::new(&mut draft.generations).range(1..=100));
                                ui.end_row();
                            });
                            ui.weak("关闭后保留已有副本；正在写入的任务会完成。数量限制在下次成功备份时应用。");
                            if ui.add_enabled(draft != policy, theme::primary_button("保存设置", light)).clicked() {
                                chosen = Some(Action::Configure(draft.clone()));
                            }
                            if draft == policy { ui.weak("当前设置已应用"); }
                            return;
                        }
                        for error in &self.catalog.errors {
                            ui.label(format!("部分副本无法读取：{error}"));
                        }
                        if self.catalog.entries.is_empty() && self.pending.is_none() {
                            ui.weak(if policy.enabled { format!("暂无恢复副本。有修改的画布会每 {} 秒自动备份。", policy.interval_seconds) } else { "暂无恢复副本。自动备份已关闭。".into() });
                        }
                        for (directory, entry) in &self.catalog.entries {
                            ui.separator();
                            let title = entry
                                .source
                                .as_ref()
                                .and_then(|path| path.file_name())
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "未命名画布".into());
                            ui.label(title);
                            if let Some(source) = &entry.source {
                                ui.weak(source.to_string_lossy())
                                    .on_hover_text(source.display().to_string());
                            }
                            let age = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|now| {
                                    now.as_millis().saturating_sub(entry.created_ms) / 60_000
                                })
                                .unwrap_or(0);
                            ui.weak(format!("约 {age} 分钟前 · 修订 {}", entry.revision));
                            ui.add_enabled_ui(self.pending.is_none(), |ui| {
                                if self.confirm_delete.as_ref() == Some(directory) {
                                    ui.label("删除这份恢复副本？此操作无法撤销，原文件不受影响。");
                                    ui.horizontal(|ui| {
                                        if ui.button("确认删除此副本").clicked() { delete = Some(directory.clone()); }
                                        if ui.button("取消").clicked() { self.confirm_delete = None; }
                                    });
                                } else {
                                    ui.horizontal(|ui| {
                                        if ui.add(theme::primary_button("恢复为新草稿", light)).clicked() {
                                            match recovery::document_path(directory) {
                                                Ok(path) => {
                                                    chosen = Some(Action::Restore(path));
                                                    self.open = false;
                                                }
                                                Err(error) => restore_error = Some(error.to_string()),
                                            }
                                        }
                                        if ui.button("删除副本…").clicked() { self.confirm_delete = Some(directory.clone()); }
                                    });
                                }
                            });
                        }
                    });
            });
        if let Some(error) = restore_error {
            self.catalog.errors.push(error);
        }
        if let Some(path) = delete {
            self.scan(Some(path));
        } else if refresh {
            self.refresh();
        }
        chosen
    }
}

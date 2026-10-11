use super::*;
use bevy_egui::{EguiContexts, egui};
use project_graph_bevy_canvas::collaboration::{self as ai, canvas::Bindings, skills::RunRef};
use std::{
    path::PathBuf,
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

type Completion = (ai::Result<ai::Snapshot>, Option<Pending>);
struct Pending {
    ids: Vec<String>,
    anchor: [f32; 2],
    document_token: uuid::Uuid,
    auto_start: bool,
}

#[derive(Resource)]
pub struct State {
    runtime: Option<ai::Runtime>,
    snapshot: ai::Snapshot,
    open: bool,
    project: String,
    paths: String,
    test_command: String,
    model: String,
    key: String,
    receiver: Mutex<Option<mpsc::Receiver<Completion>>>,
    busy: bool,
    auto_start: HashSet<String>,
    owners: HashMap<String, uuid::Uuid>,
    sync_errors: HashMap<uuid::Uuid, String>,
    poll_error: Option<String>,
    binding_error: Option<(uuid::Uuid, String)>,
    last_poll: Instant,
    scope: Vec<RunRef>,
    skill_name: String,
    draft: String,
    draft_run: Option<RunRef>,
    notice: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            runtime: None,
            snapshot: ai::Snapshot::default(),
            open: false,
            project: String::new(),
            paths: String::new(),
            test_command: "[]".into(),
            model: "deepseek/deepseek-flash".into(),
            key: String::new(),
            receiver: Mutex::new(None),
            busy: false,
            auto_start: default(),
            owners: default(),
            sync_errors: default(),
            poll_error: None,
            binding_error: None,
            last_poll: Instant::now(),
            scope: vec![],
            skill_name: "code-workflow".into(),
            draft: String::new(),
            draft_run: None,
            notice: String::new(),
        }
    }
}
impl State {
    fn runtime(&mut self) -> ai::Result<ai::Runtime> {
        if let Some(runtime) = &self.runtime {
            return Ok(runtime.clone());
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("找不到用户数据目录")?;
        let directory = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else if cfg!(target_os = "windows") {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .ok_or("找不到 LOCALAPPDATA")?
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share"))
        };
        let runtime = ai::Runtime::open(directory.join("ravomi/code-collaboration-canvas"))?;
        self.snapshot = runtime.snapshot()?;
        self.runtime = Some(runtime.clone());
        Ok(runtime)
    }
    fn authorize(&self, token: uuid::Uuid, command: &ai::Command) -> ai::Result<()> {
        let owns = |id: &str| self.owners.get(id) == Some(&token);
        let permitted = match command {
            ai::Command::Create { .. } => true,
            ai::Command::CreateSkill { sources, .. } => {
                !sources.is_empty() && sources.iter().all(|source| owns(&source.workflow_id))
            }
            ai::Command::SetPlan { id, .. }
            | ai::Command::Start { id }
            | ai::Command::Pause { id }
            | ai::Command::Stop { id, .. }
            | ai::Command::Retry { id, .. }
            | ai::Command::Resolve { id, .. }
            | ai::Command::Review { id, .. }
            | ai::Command::Test { id, .. } => owns(id),
            ai::Command::Flush => self.owners.values().any(|owner| *owner == token),
        };
        if permitted {
            Ok(())
        } else {
            Err("此流程未连接到当前文档，不能控制其他文档的运行".into())
        }
    }

    fn owned_runtime(&mut self, token: uuid::Uuid, workflow: &str) -> ai::Result<ai::Runtime> {
        if self.owners.get(workflow) != Some(&token) {
            return Err("此流程未连接到当前文档，不能读取其执行权限".into());
        }
        self.runtime()
    }

    fn connected_snapshot(
        &self,
        doc: &Document,
        snapshot: &ai::Snapshot,
    ) -> ai::Result<ai::Snapshot> {
        let bindings = Bindings::read(doc)?;
        let token = doc.instance_token();
        Ok(ai::Snapshot {
            version: snapshot.version,
            workflows: snapshot
                .workflows
                .iter()
                .filter(|workflow| {
                    self.owners.get(&workflow.id) == Some(&token)
                        && bindings
                            .workflows
                            .iter()
                            .any(|b| b.workflow_id == workflow.id)
                })
                .cloned()
                .collect(),
            storage_error: snapshot.storage_error.clone(),
        })
    }

    /// One fixed Runtime view is shared by every document in a recovery batch.
    pub(super) fn snapshot_for_save(&mut self) -> ai::Result<ai::Snapshot> {
        if let Some(runtime) = &self.runtime {
            self.snapshot = runtime.snapshot()?;
        }
        Ok(self.snapshot.clone())
    }

    pub(super) fn prepare_document(
        &self,
        doc: &mut Document,
        snapshot: &ai::Snapshot,
    ) -> ai::Result<()> {
        let selected = self.connected_snapshot(doc, snapshot)?;
        if !selected.workflows.is_empty() {
            Bindings::read(doc)?.sync(doc, &selected)?;
        }
        Ok(())
    }

    pub(super) fn prepare_save(&mut self, doc: &mut Document) -> ai::Result<()> {
        let snapshot = self.snapshot_for_save()?;
        self.prepare_document(doc, &snapshot)
    }

    fn connect(&mut self, doc: &mut Document) -> ai::Result<usize> {
        let snapshot = self.runtime()?.snapshot()?;
        let count = self.connect_snapshot(doc, &snapshot)?;
        self.snapshot = snapshot;
        Ok(count)
    }

    fn connect_snapshot(
        &mut self,
        doc: &mut Document,
        snapshot: &ai::Snapshot,
    ) -> ai::Result<usize> {
        let mut bindings = Bindings::read(doc)?;
        let token = doc.instance_token();
        let selected = ai::Snapshot {
            version: snapshot.version,
            workflows: snapshot
                .workflows
                .iter()
                .filter(|w| bindings.workflows.iter().any(|b| b.workflow_id == w.id))
                .cloned()
                .collect(),
            storage_error: snapshot.storage_error.clone(),
        };
        if selected.workflows.is_empty() {
            return Err("未找到绑定对应的本机流程；项目内已有归档保持不变".into());
        }
        if selected
            .workflows
            .iter()
            .any(|w| self.owners.get(&w.id).is_some_and(|owner| *owner != token))
        {
            return Err("流程仍连接到另一个打开的文档；副本不能接管其运行".into());
        }
        bindings.sync(doc, &selected)?;
        for workflow in &selected.workflows {
            self.owners.insert(workflow.id.clone(), token);
        }
        Ok(selected.workflows.len())
    }

    fn dispatch(
        &mut self,
        token: uuid::Uuid,
        command: ai::Command,
        pending: Option<Pending>,
    ) -> ai::Result<()> {
        self.authorize(token, &command)?;
        if self.busy {
            return Err("正在保存上一次操作，请稍候".into());
        }
        let runtime = self.runtime()?;
        let (tx, rx) = mpsc::channel();
        *self.receiver.get_mut().map_err(|e| e.to_string())? = Some(rx);
        self.busy = true;
        std::thread::spawn(move || {
            let _ = tx.send((runtime.command(command), pending));
        });
        Ok(())
    }
    fn poll(&mut self, doc: &mut Document, tabs: &mut tabs::Tabs) -> ai::Result<bool> {
        let mut updated = false;
        let completion = self
            .receiver
            .get_mut()
            .map_err(|e| e.to_string())?
            .as_ref()
            .map(|r| r.try_recv());
        if let Some(Ok((result, pending))) = completion {
            self.busy = false;
            *self.receiver.get_mut().map_err(|e| e.to_string())? = None;
            let snapshot = result?;
            if let Some(pending) = pending {
                let w = snapshot.workflows.last().ok_or("未创建流程")?;
                if let Some(target) = tabs.document_mut(doc, pending.document_token) {
                    let mut bindings = Bindings::read(target)?;
                    bindings.workflows.push(ai::canvas::Binding {
                        workflow_id: w.id.clone(),
                        input_ids: pending.ids,
                        anchor: pending.anchor,
                        agents: vec![],
                    });
                    let mut selected = self.connected_snapshot(target, &snapshot)?;
                    selected.workflows.retain(|workflow| workflow.id != w.id);
                    selected.workflows.push(w.clone());
                    bindings.sync(target, &selected)?;
                    self.owners.insert(w.id.clone(), pending.document_token);
                    if pending.auto_start {
                        self.auto_start.insert(w.id.clone());
                    }
                } else {
                    self.notice =
                        "任务已创建，但原文档已关闭；本机记录已保留，未自动开始代码执行".into();
                }
            }
            self.snapshot = snapshot;
            updated = true;
        } else if matches!(completion, Some(Err(mpsc::TryRecvError::Disconnected))) {
            self.busy = false;
            *self.receiver.get_mut().map_err(|e| e.to_string())? = None;
            return Err("AI 操作线程异常退出".into());
        }
        if self.last_poll.elapsed() >= Duration::from_millis(300) {
            self.last_poll = Instant::now();
            if let Some(runtime) = &self.runtime {
                self.snapshot = runtime.snapshot()?;
                updated = true;
            }
        }
        if !self.busy {
            for w in &self.snapshot.workflows {
                if self.auto_start.contains(&w.id) {
                    if let Some(plan) = &w.draft_plan {
                        let command = if w.plan.is_none() {
                            ai::Command::SetPlan {
                                id: w.id.clone(),
                                plan: plan.clone(),
                            }
                        } else {
                            self.auto_start.remove(&w.id);
                            ai::Command::Start { id: w.id.clone() }
                        };
                        let Some(token) = self.owners.get(&w.id).copied() else {
                            self.auto_start.remove(&w.id);
                            continue;
                        };
                        self.dispatch(token, command, None)?;
                        break;
                    } else if w
                        .runs
                        .iter()
                        .any(|r| matches!(r.status, ai::Status::Failed | ai::Status::Cancelled))
                    {
                        self.auto_start.remove(&w.id);
                    }
                }
            }
        }
        Ok(updated)
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(runtime) = &self.runtime
            && let Err(error) = runtime.shutdown()
        {
            eprintln!("停止画布 AI 失败：{error}");
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn ui(
    mut contexts: EguiContexts,
    mut state: ResMut<State>,
    mut scene: ResMut<Scene>,
    interaction: Res<Interaction>,
    mut editor: ResMut<editor::Editor>,
    mut display: ResMut<sections::Display>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut tabs: ResMut<tabs::Tabs>,
    config: Res<Config>,
) -> bevy::ecs::error::Result {
    if config.no_ui {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let instances = tabs.instance_tokens(&scene.0);
    state.owners.retain(|_, owner| instances.contains(owner));
    state
        .sync_errors
        .retain(|token, _| instances.contains(token));
    let owned: HashSet<_> = state.owners.keys().cloned().collect();
    state.auto_start.retain(|id| owned.contains(id));
    if editor.active.is_none() && !scene.0.in_transaction() {
        match state.poll(&mut scene.0, &mut tabs) {
            Ok(updated) => {
                if updated {
                    state.poll_error = None;
                    tabs.for_each_document_mut(&mut scene.0, |doc| {
                        let token = doc.instance_token();
                        match state.prepare_document(doc, &state.snapshot) {
                            Ok(()) => {
                                state.sync_errors.remove(&token);
                            }
                            Err(error) => {
                                if state.sync_errors.get(&token) != Some(&error) {
                                    editor.report_error(error.clone());
                                }
                                state.sync_errors.insert(token, error);
                            }
                        }
                        Ok(())
                    })?;
                }
            }
            Err(error) => {
                if state.poll_error.as_ref() != Some(&error) {
                    editor.report_error(error.clone());
                }
                state.poll_error = Some(error);
            }
        }
    }
    let token = scene.0.instance_token();
    let bindings = match Bindings::read(&scene.0) {
        Ok(b) => {
            state.binding_error = None;
            b
        }
        Err(e) => {
            let error = (token, e);
            if state.binding_error.as_ref() != Some(&error) {
                editor.report_error(error.1.clone());
            }
            state.binding_error = Some(error);
            return Ok(());
        }
    };
    if !bindings.workflows.is_empty() {
        display.enabled = true;
    }
    let connected = match state.connected_snapshot(&scene.0, &state.snapshot) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            editor.report_error(error);
            return Ok(());
        }
    };
    state.scope.retain(|source| {
        connected
            .workflows
            .iter()
            .any(|w| w.id == source.workflow_id)
    });
    let submit_key = keys.just_pressed(KeyCode::Space)
        && !editor.capture
        && !ctx.egui_wants_keyboard_input()
        && !mouse.any_pressed([MouseButton::Left, MouseButton::Middle, MouseButton::Right])
        && !keys.any_pressed([
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ])
        && windows.single().is_ok_and(|w| w.focused)
        && !interaction.selected.is_empty();
    let mut submit = submit_key;
    if !editor.home_visible() {
        egui::Area::new("canvas-ai-entry".into())
            .anchor(egui::Align2::RIGHT_TOP, [-20.0, 86.0])
            .show(ctx, |ui| {
                theme::chrome_style(ui, editor.light);
                let colors = theme::chrome_palette(editor.light);
                let response = ui.add(
                    egui::Button::new((
                        egui::Atom::grow(),
                        egui::RichText::new("协作").color(if state.open {
                            colors.ink
                        } else {
                            colors.foreground
                        }),
                        egui::Atom::grow(),
                    ))
                    .min_size(egui::vec2(80.0, 36.0))
                    .corner_radius(18)
                    .selected(state.open)
                    .fill(if state.open {
                        colors.active
                    } else {
                        colors.surface
                    }),
                );
                if response
                    .on_hover_text("打开画布协作 · Pi 引擎\n选中文本或分组，按 Space 提交任务")
                    .clicked()
                {
                    state.open = !state.open;
                }
            });
    }
    let selected_runs = bindings.selected_runs(&scene.0, &interaction.selected, &connected);
    let mut command = None;
    let mut new_skill = false;
    let mut connect = false;
    let mut open = state.open || submit_key;
    let mut close = false;
    if open && !editor.home_visible() {
        egui::Window::new("画布协作").title_bar(false).frame(theme::tool_frame(editor.light)).open(&mut open).default_width(430.0).min_width(120.0).max_width((ctx.content_rect().width() - 64.0).max(120.0)).default_pos([700.0, 135.0]).show(ctx, |ui| {
            theme::tool_style(ui, editor.light);
            ui.spacing_mut().scroll.fade.strength = 0.0;
            close = theme::tool_header(ui, "画布协作", editor.light);
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 140.0).clamp(80.0, 650.0)).show(ui, |ui| {
                ui.label("选区是输入；Space 开始任务。就绪的 Agent 可并行运行。");
                if bindings.workflows.iter().any(|b| state.owners.get(&b.workflow_id) != Some(&token)) {
                    ui.label("项目内历史保持为归档；连接本机流程后才接收更新与控制运行。");
                    if ui.add_enabled(!state.busy && editor.active.is_none() && !scene.0.in_transaction(), egui::Button::new("连接本机已保存流程")).clicked() { connect = true; }
                }
                ui.weak("选区文字直接作为输入；运行记录与成果随当前 .mio 保存。模型仍需有效连接配置。");
                ui.collapsing("模型与可选代码附件", |ui| {
                    ui.label("外部代码项目目录（可留空）"); ui.text_edit_singleline(&mut state.project);
                    ui.label("附加代码路径（可留空；每行一项，相对外部项目目录）");
                    ui.add(egui::TextEdit::multiline(&mut state.paths).desired_rows(3));
                    ui.label("授权的测试命令 argv（JSON 数组；空数组不运行测试）");
                    ui.text_edit_singleline(&mut state.test_command);
                    ui.label("模型 provider/model"); ui.text_edit_singleline(&mut state.model);
                    ui.label("API Key（仅本次进程；留空使用现有环境配置）");
                    ui.add(egui::TextEdit::singleline(&mut state.key).password(true));
                    if ui.button("应用连接配置").clicked() {
                        let provider = state.model.split('/').next().unwrap_or("").to_string();
                        let key = std::mem::take(&mut state.key);
                        let result = state.runtime().and_then(|r| if key.is_empty() { Ok(()) } else { r.configure(&provider, key) });
                        match result { Ok(()) => state.notice = "连接配置已应用，实际请求时验证".into(), Err(e) => editor.report_error(e) }
                    }
                });
                if ui.add_enabled(!state.busy, egui::Button::new("执行选中文本 / 分组")).clicked() { submit = true; }
                ui.collapsing("按列表选择 Agent（键盘可用）", |ui| {
                    for binding in &bindings.workflows {
                        if let Some(w) = connected.workflows.iter().find(|w| w.id == binding.workflow_id).cloned() {
                            for agent in &binding.agents {
                                let refs: Vec<_> = w.runs.iter().filter(|r| r.task_id == agent.task_id)
                                    .map(|r| RunRef { workflow_id: w.id.clone(), run_id: r.id.clone() }).collect();
                                if refs.is_empty() { continue; }
                                let mut checked = refs.iter().all(|r| state.scope.contains(r));
                                let title = scene.0.nodes.iter().find(|n| n.id == agent.group_id).map(|n| n.text.clone()).unwrap_or_else(|| agent.task_id.clone());
                                if ui.checkbox(&mut checked, title).changed() {
                                    state.scope.retain(|r| !refs.contains(r));
                                    if checked { state.scope.extend(refs); }
                                }
                            }
                        }
                    }
                });
                if !selected_runs.is_empty() {
                    ui.separator();
                    ui.label(format!("选中 Agent 的 {} 次运行", selected_runs.len()));
                    if ui.button("展开 / 折叠选中节点").clicked() {
                        let collapse = interaction.selected.iter().any(|&i| scene.0.nodes.get(i).is_some_and(|n| n.kind == "Section" && !n.collapsed));
                        if let Err(e) = scene.0.set_sections_collapsed(&interaction.selected, collapse) { editor.report_error(e.to_string()); }
                    }
                    if ui.button("总结选区为 Skill…").clicked() { state.scope = selected_runs.clone(); }
                }
                if !state.scope.is_empty() {
                    ui.separator(); ui.heading("Skill 来源预览");
                    ui.label("可取消纳入某次运行；生成时固定证据，之后完成的结果不会混入。");
                    let snapshot = &connected;
                    state.scope.retain(|reference| {
                        let run = snapshot.workflows.iter().find(|w| w.id == reference.workflow_id).and_then(|w| w.runs.iter().find(|r| r.id == reference.run_id));
                        let mut checked = true;
                        ui.checkbox(&mut checked, run.map(|r| format!("{} · {} · {}", r.task_id, ai::canvas::status(&r.status), r.id)).unwrap_or_else(|| "来源已缺失".into())); checked
                    });
                    ui.label("Skill 名称（小写英文及连字符）"); ui.text_edit_singleline(&mut state.skill_name);
                    if ui.add_enabled(!state.busy, egui::Button::new("生成工作流 Skill 草稿")).clicked() { new_skill = true; }
                    if ui.button("取消整理").clicked() { state.scope.clear(); }
                }
                if state.draft_run.as_ref().is_some_and(|r| connected.workflows.iter().any(|w| w.id == r.workflow_id)) {
                    ui.separator(); ui.heading("审阅并编辑 SKILL.md");
                    ui.add(egui::TextEdit::multiline(&mut state.draft).desired_rows(14).code_editor());
                    ui.label("将新增项目内 .agents/skills/<name>/SKILL.md 与来源记录；不覆盖现有文件。");
                    if ui.button("确认审阅并保存到项目").clicked() {
                        let reference = state.draft_run.clone().unwrap();
                        let content = state.draft.clone();
                        match state.owned_runtime(token, &reference.workflow_id).and_then(|r| r.save_skill(&reference.workflow_id, &reference.run_id, &content)) {
                            Ok(path) => { state.notice = format!("已保存：{}", path.display()); state.draft_run = None; },
                            Err(e) => editor.report_error(e),
                        }
                    }
                    if ui.button("放弃此编辑草稿").clicked() { state.draft_run = None; }
                }
                ui.separator(); ui.heading("运行记录");
                for w in connected.workflows.iter().rev() {
                    if let Some(run) = w.runs.last().filter(|run| run.status == ai::Status::Failed) {
                        ui.label(format!("{}：{}失败", w.goal.chars().take(30).collect::<String>(), if run.task_id == "$plan" { "任务规划" } else { "运行" }));
                        if let Some(error) = &run.error { ui.label(error); }
                        if ui.add_enabled(!state.busy, egui::Button::new("配置完成后重试")).clicked() {
                            command = Some(ai::Command::Retry { id: w.id.clone(), run_id: run.id.clone() });
                        }
                    }
                    ui.collapsing(format!("{} · {}", w.goal.chars().take(30).collect::<String>(), &w.id[..8]), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("暂停派发").clicked() { state.auto_start.remove(&w.id); command = Some(ai::Command::Pause { id: w.id.clone() }); }
                            if ui.button("停止全部").clicked() { state.auto_start.remove(&w.id); command = Some(ai::Command::Stop { id: w.id.clone(), task_id: None }); }
                            if w.plan.is_some() && ui.button("继续").clicked() { command = Some(ai::Command::Start { id: w.id.clone() }); }
                            if w.plan.is_none() && let Some(plan) = &w.draft_plan && ui.button("采纳计划").clicked() { command = Some(ai::Command::SetPlan { id: w.id.clone(), plan: plan.clone() }); }
                        });
                        for run in &w.runs {
                            ui.collapsing(format!("{} · {} · {}", run.task_id, ai::canvas::status(&run.status), &run.id[..8]), |ui| {
                                ui.label(format!("Run {}", run.id));
                                if let Some(e) = &run.error { ui.label(e); }
                                if matches!(run.status, ai::Status::Running | ai::Status::Queued) && ui.button("停止此分支").clicked() {
                                    state.auto_start.remove(&w.id);
                                    command = Some(ai::Command::Stop { id: w.id.clone(), task_id: Some(run.task_id.clone()) });
                                }
                                ui.add(egui::TextEdit::multiline(&mut run.output.as_str()).desired_rows(5).code_editor());
                                if run.task_id != "$skill" && run.status == ai::Status::Succeeded && run.proposal.is_some() {
                                    if !w.test_command.is_empty() && ui.button("在隔离副本中运行已授权测试").clicked() {
                                        command = Some(ai::Command::Test { id: w.id.clone(), run_id: run.id.clone() });
                                    }
                                    if ui.button("审阅通过此版本").clicked() { command = Some(ai::Command::Review { id: w.id.clone(), run_id: run.id.clone(), decision: "approved".into() }); }
                                    if run.review.as_deref() == Some("approved") && ui.button("导出代码到新目录…").clicked()
                                        && let Some(path) = rfd::FileDialog::new().set_title("选择父目录，将在其中新建独立代码目录").pick_folder() {
                                        match state.owned_runtime(token, &w.id).and_then(|r| r.export(&w.id, &run.id, &path)) {
                                            Ok(message) => state.notice = message, Err(e) => editor.report_error(e),
                                        }
                                    }
                                }
                                if matches!(run.status, ai::Status::Failed | ai::Status::Cancelled) && ui.button("固定输入重试").clicked() { command = Some(ai::Command::Retry { id: w.id.clone(), run_id: run.id.clone() }); }
                                if run.status == ai::Status::Unknown && ui.button("已核对旧进程结束").clicked() { command = Some(ai::Command::Resolve { id: w.id.clone(), run_id: run.id.clone() }); }
                                if run.task_id == "$skill" && run.status == ai::Status::Succeeded && ui.button("打开 Skill 草稿").clicked() && let Some(proposal) = &run.proposal {
                                    state.draft = proposal.files[0].content.clone(); state.draft_run = Some(RunRef { workflow_id: w.id.clone(), run_id: run.id.clone() });
                                }
                            });
                        }
                    });
                }
                let archived = scene.0.history_archive();
                for workflow in archived.workflows.iter().filter(|w| !connected.workflows.iter().any(|live| live.id == w.id)) {
                    ui.collapsing(format!("归档 · {}", workflow.goal.chars().take(30).collect::<String>()), |ui| {
                        ui.weak("固定采集的历史；不会恢复执行或授予本机运行权限。");
                        for run in &workflow.runs {
                            ui.collapsing(format!("{} · {:?} · {}", run.task_id, run.status, run.id), |ui| {
                                ui.add(egui::TextEdit::multiline(&mut run.output.as_str()).desired_rows(5).code_editor());
                                if let Some(error) = &run.error { ui.label(error); }
                            });
                        }
                    });
                }
                if !connected.workflows.is_empty() && connected.storage_error.is_some() && ui.button("重试保存运行记录").clicked() { command = Some(ai::Command::Flush); }
                if !state.notice.is_empty() { ui.separator(); ui.label(&state.notice); }
            });
        });
    }
    state.open = open && !close;
    if connect {
        match state.connect(&mut scene.0) {
            Ok(count) => state.notice = format!("已连接 {count} 个本机流程；其他归档保持不变"),
            Err(error) => editor.report_error(error),
        }
    }
    if submit && !state.busy {
        let result = (|| {
            let (goal, ids, anchor) = bindings.capture(&scene.0, &interaction.selected)?;
            if bindings.workflows.iter().any(|b| {
                b.input_ids == ids
                    && connected.workflows.iter().any(|w| {
                        w.id == b.workflow_id
                            && w.runs.iter().any(|r| {
                                matches!(
                                    r.status,
                                    ai::Status::Running | ai::Status::Queued | ai::Status::Stopping
                                )
                            })
                    })
            }) {
                return Err("此输入已有执行中的任务，请先等待或停止".into());
            }
            let command = ai::Command::Create {
                project: state.project.clone(),
                paths: state
                    .paths
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                goal,
                model: state.model.clone(),
                test_command: serde_json::from_str(&state.test_command)
                    .map_err(|e| format!("测试命令需要 JSON argv 数组：{e}"))?,
            };
            state.dispatch(
                token,
                command,
                Some(Pending {
                    ids,
                    anchor,
                    document_token: token,
                    auto_start: true,
                }),
            )
        })();
        if let Err(e) = result {
            editor.report_error(e);
        }
    }
    if new_skill {
        let command = ai::Command::CreateSkill {
            project: state.project.clone(),
            model: state.model.clone(),
            name: state.skill_name.clone(),
            sources: state.scope.clone(),
        };
        let anchor = interaction
            .selected
            .iter()
            .filter_map(|&i| scene.0.nodes.get(i))
            .next()
            .map(|n| [n.rect[0] + n.rect[2] + 200.0, n.rect[1]])
            .unwrap_or([0.0, 0.0]);
        if let Err(e) = state.dispatch(
            token,
            command,
            Some(Pending {
                ids: vec![],
                anchor,
                document_token: token,
                auto_start: false,
            }),
        ) {
            editor.report_error(e);
        }
    } else if let Some(command) = command
        && let Err(e) = state.dispatch(token, command, None)
    {
        editor.report_error(e);
    }
    editor.capture |= ctx.egui_wants_pointer_input() || ctx.egui_wants_keyboard_input();
    editor.navigation_capture |= ctx.egui_wants_pointer_input();
    if state.runtime.is_some() {
        ctx.request_repaint_after(Duration::from_millis(300));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(id: &str, output: &str) -> ai::Snapshot {
        ai::Snapshot {
            workflows: vec![ai::Workflow {
                id: id.into(),
                goal: "固定选中的任务".into(),
                project: "/local/source".into(),
                model: "test/model".into(),
                files: vec![],
                test_command: vec![],
                plan: None,
                draft_plan: None,
                runs: vec![ai::Run {
                    id: format!("{id}-run"),
                    task_id: "$plan".into(),
                    status: ai::Status::Running,
                    created_at: 1,
                    finished_at: None,
                    sources: vec![],
                    prompt: "fixed prompt".into(),
                    input_files: vec![],
                    output: output.into(),
                    proposal: None,
                    error: None,
                    review: None,
                    usage: None,
                    session_id: None,
                    retry_of: None,
                }],
                paused: true,
                max_calls: 2,
                timeout_seconds: 60,
            }],
            ..default()
        }
    }

    fn bind(doc: &mut Document, id: &str) {
        let mut bindings = Bindings::read(doc).unwrap();
        bindings.workflows.push(ai::canvas::Binding {
            workflow_id: id.into(),
            input_ids: vec![],
            anchor: [0.0; 2],
            agents: vec![],
        });
        bindings.write(doc).unwrap();
    }

    #[test]
    fn only_connected_instance_updates_evidence_and_controls_runs() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.mio");
        let mut doc = Document::blank().unwrap();
        bind(&mut doc, "selected");
        bind(&mut doc, "foreign");
        let mut state = State::default();
        state.owners.insert("selected".into(), doc.instance_token());
        let initial = snapshot("selected", "first output");
        state.prepare_document(&mut doc, &initial).unwrap();
        doc.save(&path).unwrap();
        let mut reopened = Document::open(&path).unwrap();
        let revision = doc.revision;
        state.prepare_document(&mut doc, &initial).unwrap();
        assert!(!doc.dirty());
        assert_eq!(doc.revision, revision);

        let long_output = "全文不能被预览截断。".repeat(400);
        let mut fresh = snapshot("selected", &long_output);
        fresh
            .workflows
            .extend(snapshot("foreign", "not selected").workflows);
        state.prepare_document(&mut doc, &fresh).unwrap();
        assert!(doc.dirty());
        assert_eq!(doc.history_archive().workflows.len(), 1);
        assert_eq!(
            doc.history_archive().workflows[0].runs[0].output,
            long_output
        );
        assert!(
            doc.nodes
                .iter()
                .any(|node| node.text.contains("完整记录见 AI 详情"))
        );

        state.prepare_document(&mut reopened, &fresh).unwrap();
        assert!(!reopened.dirty());
        assert_eq!(
            reopened.history_archive().workflows[0].runs[0].output,
            "first output"
        );
        let stop = ai::Command::Stop {
            id: "selected".into(),
            task_id: None,
        };
        assert!(state.authorize(doc.instance_token(), &stop).is_ok());
        assert!(state.authorize(reopened.instance_token(), &stop).is_err());
        assert!(state.connect_snapshot(&mut reopened, &fresh).is_err());
        assert!(state.runtime.is_none());

        let recovery = directory.path().join("recovery.mio");
        doc.capture_snapshot()
            .unwrap()
            .save_snapshot(&recovery)
            .unwrap();
        let restored = Document::open_recovery(&recovery).unwrap();
        assert_eq!(restored.history_archive(), doc.history_archive());
        assert!(state.authorize(restored.instance_token(), &stop).is_err());
        assert!(doc.dirty());
    }

    #[test]
    fn missing_runtime_keeps_saved_history_and_binding_only_changes_are_dirty() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("saved.mio");
        let mut doc = Document::blank().unwrap();
        doc.save(&path).unwrap();
        bind(&mut doc, "local");
        assert!(doc.dirty());
        let mut state = State::default();
        let captured = snapshot("local", "saved evidence");
        state.connect_snapshot(&mut doc, &captured).unwrap();
        doc.save(&path).unwrap();
        let archived = doc.history_archive().clone();
        let revision = doc.revision;
        state
            .prepare_document(&mut doc, &ai::Snapshot::default())
            .unwrap();
        assert!(
            state
                .connect_snapshot(&mut doc, &ai::Snapshot::default())
                .is_err()
        );
        assert_eq!(doc.history_archive(), &archived);
        assert_eq!(doc.revision, revision);
        assert!(!doc.dirty());
    }

    #[test]
    fn failed_history_refresh_does_not_change_saved_projection_or_archive() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("saved.mio");
        let mut doc = Document::blank().unwrap();
        bind(&mut doc, "local");
        let mut state = State::default();
        let initial = snapshot("local", "original");
        state.connect_snapshot(&mut doc, &initial).unwrap();
        doc.save(&path).unwrap();
        let texts: Vec<_> = doc.nodes.iter().map(|node| node.text.clone()).collect();
        let archived = doc.history_archive().clone();
        let mut conflicting = initial;
        conflicting.workflows[0].runs[0].prompt = "different immutable input".into();
        conflicting.workflows[0].runs[0].output = "must not appear".into();
        assert!(state.prepare_document(&mut doc, &conflicting).is_err());
        assert_eq!(doc.history_archive(), &archived);
        assert_eq!(
            doc.nodes
                .iter()
                .map(|node| node.text.clone())
                .collect::<Vec<_>>(),
            texts
        );
        assert!(!doc.dirty());
    }

    #[test]
    fn failed_agent_creation_preserves_document_and_bindings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("saved.mio");
        let mut doc = Document::blank().unwrap();
        let mut bindings = Bindings::default();
        bindings.workflows.push(ai::canvas::Binding {
            workflow_id: "bad-position".into(),
            input_ids: vec![],
            anchor: [f32::MAX, f32::MAX],
            agents: vec![],
        });
        bindings.write(&mut doc).unwrap();
        doc.save(&path).unwrap();
        // A non-finite location must fail before either archive or graph changes.
        bindings.workflows[0].anchor = [f32::INFINITY, 0.0];
        let revision = doc.revision;
        let attachments = doc.attachments.clone();
        assert!(
            bindings
                .sync(&mut doc, &snapshot("bad-position", "evidence"))
                .is_err()
        );
        assert!(doc.history_archive().workflows.is_empty());
        assert!(doc.nodes.is_empty());
        assert_eq!(doc.attachments, attachments);
        assert_eq!(doc.revision, revision);
        assert!(!doc.dirty());
        assert!(bindings.workflows[0].agents.is_empty());
    }

    #[test]
    fn creation_completion_routes_to_inactive_document_and_ignores_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inactive.mio");
        Document::blank().unwrap().save(&path).unwrap();
        let mut active = Document::blank().unwrap();
        let mut tabs = tabs::Tabs::from_args(
            &active,
            &["--tab".into(), path.to_string_lossy().into_owned()],
        )
        .unwrap();
        let token = *tabs
            .instance_tokens(&active)
            .iter()
            .find(|&&token| token != active.instance_token())
            .unwrap();
        let mut state = State::default();
        let (sender, receiver) = mpsc::channel();
        state.receiver = Mutex::new(Some(receiver));
        state.busy = true;
        sender
            .send((
                Ok(snapshot("created", "completed planning")),
                Some(Pending {
                    ids: vec![],
                    anchor: [0.0; 2],
                    document_token: token,
                    auto_start: false,
                }),
            ))
            .unwrap();
        state.poll(&mut active, &mut tabs).unwrap();
        assert!(active.history_archive().workflows.is_empty());
        let inactive = tabs.document_mut(&mut active, token).unwrap();
        assert_eq!(inactive.history_archive().workflows[0].id, "created");
        assert!(inactive.dirty());

        let old_token = active.instance_token();
        active = Document::blank().unwrap();
        let (sender, receiver) = mpsc::channel();
        state.receiver = Mutex::new(Some(receiver));
        sender
            .send((
                Ok(snapshot("orphan", "old document")),
                Some(Pending {
                    ids: vec![],
                    anchor: [0.0; 2],
                    document_token: old_token,
                    auto_start: false,
                }),
            ))
            .unwrap();
        state.poll(&mut active, &mut tabs).unwrap();
        assert!(active.history_archive().workflows.is_empty());
        assert!(!state.owners.contains_key("orphan"));
        assert!(state.notice.contains("原文档已关闭"));
    }
}

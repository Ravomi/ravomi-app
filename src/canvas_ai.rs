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
    document_id: u64,
    auto_start: bool,
}

#[derive(Resource)]
pub struct State {
    runtime: Option<ai::Runtime>,
    load_attempted: bool,
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
            load_attempted: false,
            snapshot: ai::Snapshot::default(),
            open: false,
            project: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            paths: String::new(),
            test_command: "[]".into(),
            model: "deepseek/deepseek-flash".into(),
            key: String::new(),
            receiver: Mutex::new(None),
            busy: false,
            auto_start: default(),
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
        self.load_attempted = true;
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
    fn dispatch(&mut self, command: ai::Command, pending: Option<Pending>) -> ai::Result<()> {
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
    fn poll(&mut self, doc: &mut Document, document_id: u64) -> ai::Result<()> {
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
                if pending.document_id == document_id {
                    let mut bindings = Bindings::read(doc)?;
                    bindings.workflows.push(ai::canvas::Binding {
                        workflow_id: w.id.clone(),
                        input_ids: pending.ids,
                        anchor: pending.anchor,
                        agents: vec![],
                    });
                    bindings.sync(doc, &snapshot)?;
                    if pending.auto_start {
                        self.auto_start.insert(w.id.clone());
                    }
                } else {
                    self.notice =
                        "任务已创建，但画布已切换；请在运行列表中查看，未自动开始代码执行".into();
                }
            }
            self.snapshot = snapshot;
        } else if matches!(completion, Some(Err(mpsc::TryRecvError::Disconnected))) {
            self.busy = false;
            *self.receiver.get_mut().map_err(|e| e.to_string())? = None;
            return Err("AI 操作线程异常退出".into());
        }
        if self.last_poll.elapsed() >= Duration::from_millis(300) {
            if let Some(runtime) = &self.runtime {
                self.snapshot = runtime.snapshot()?;
            }
            self.last_poll = Instant::now();
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
                        self.dispatch(command, None)?;
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
        Ok(())
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
    tabs: Res<tabs::Tabs>,
    config: Res<Config>,
) -> bevy::ecs::error::Result {
    if config.no_ui {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    if editor.active.is_none()
        && !scene.0.in_transaction()
        && let Err(error) = state.poll(&mut scene.0, tabs.active_id())
    {
        editor.report_error(error);
    }
    let mut bindings = match Bindings::read(&scene.0) {
        Ok(b) => b,
        Err(e) => {
            editor.report_error(e);
            return Ok(());
        }
    };
    if !bindings.workflows.is_empty() {
        display.enabled = true;
        if state.runtime.is_none()
            && !state.load_attempted
            && let Err(e) = state.runtime()
        {
            editor.report_error(e);
        }
        if editor.active.is_none()
            && let Err(error) = bindings.sync(&mut scene.0, &state.snapshot)
        {
            editor.report_error(error);
        }
    }
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
    egui::Area::new("canvas-ai-entry".into())
        .anchor(egui::Align2::RIGHT_TOP, [-20.0, 86.0])
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(170.0);
                if ui.button("Pi · 画布协作").clicked() {
                    state.open = !state.open;
                }
                if !interaction.selected.is_empty() {
                    ui.small("选中文本 / 分组 · Space 执行");
                }
            });
        });
    let selected_runs = bindings.selected_runs(&scene.0, &interaction.selected, &state.snapshot);
    let mut command = None;
    let mut new_skill = false;
    let mut open = state.open || submit_key;
    if open {
        egui::Window::new("画布 AI 协作").open(&mut open).default_width(430.0).default_pos([700.0, 135.0]).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(650.0).show(ui, |ui| {
                ui.label("选区是输入；Space 授权规划并执行。最多两个 Agent 同时运行。");
                ui.collapsing("项目与模型", |ui| {
                    ui.label("项目目录"); ui.text_edit_singleline(&mut state.project);
                    ui.label("允许读取的代码路径（每行一项，相对项目目录）");
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
                        if let Some(w) = state.snapshot.workflows.iter().find(|w| w.id == binding.workflow_id).cloned() {
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
                    let snapshot = state.snapshot.clone();
                    state.scope.retain(|reference| {
                        let run = snapshot.workflows.iter().find(|w| w.id == reference.workflow_id).and_then(|w| w.runs.iter().find(|r| r.id == reference.run_id));
                        let mut checked = true;
                        ui.checkbox(&mut checked, run.map(|r| format!("{} · {} · {}", r.task_id, ai::canvas::status(&r.status), r.id)).unwrap_or_else(|| "来源已缺失".into())); checked
                    });
                    ui.label("Skill 名称（小写英文及连字符）"); ui.text_edit_singleline(&mut state.skill_name);
                    if ui.add_enabled(!state.busy, egui::Button::new("生成工作流 Skill 草稿")).clicked() { new_skill = true; }
                    if ui.button("取消整理").clicked() { state.scope.clear(); }
                }
                if state.draft_run.is_some() {
                    ui.separator(); ui.heading("审阅并编辑 SKILL.md");
                    ui.add(egui::TextEdit::multiline(&mut state.draft).desired_rows(14).code_editor());
                    ui.label("将新增项目内 .agents/skills/<name>/SKILL.md 与来源记录；不覆盖现有文件。");
                    if ui.button("确认审阅并保存到项目").clicked() {
                        let reference = state.draft_run.clone().unwrap();
                        let content = state.draft.clone();
                        match state.runtime().and_then(|r| r.save_skill(&reference.workflow_id, &reference.run_id, &content)) {
                            Ok(path) => { state.notice = format!("已保存：{}", path.display()); state.draft_run = None; },
                            Err(e) => editor.report_error(e),
                        }
                    }
                    if ui.button("放弃此编辑草稿").clicked() { state.draft_run = None; }
                }
                ui.separator(); ui.heading("运行记录");
                let snapshot = state.snapshot.clone();
                for w in snapshot.workflows.iter().rev() {
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
                                        match state.runtime().and_then(|r| r.export(&w.id, &run.id, &path)) {
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
                if state.snapshot.storage_error.is_some() && ui.button("重试保存运行记录").clicked() { command = Some(ai::Command::Flush); }
                if !state.notice.is_empty() { ui.separator(); ui.label(&state.notice); }
            });
        });
    }
    state.open = open;
    if submit && !state.busy {
        let result = (|| {
            let (goal, ids, anchor) = bindings.capture(&scene.0, &interaction.selected)?;
            if state.paths.trim().is_empty() {
                state.open = true;
                return Err(
                    "请在项目与模型中指定项目目录和允许读取的代码文件，再按 Space 执行".into(),
                );
            }
            if bindings.workflows.iter().any(|b| {
                b.input_ids == ids
                    && state.snapshot.workflows.iter().any(|w| {
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
                command,
                Some(Pending {
                    ids,
                    anchor,
                    document_id: tabs.active_id(),
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
            command,
            Some(Pending {
                ids: vec![],
                anchor,
                document_id: tabs.active_id(),
                auto_start: false,
            }),
        ) {
            editor.report_error(e);
        }
    } else if let Some(command) = command
        && let Err(e) = state.dispatch(command, None)
    {
        editor.report_error(e);
    }
    editor.capture |= ctx.egui_wants_pointer_input() || ctx.egui_wants_keyboard_input();
    if state.runtime.is_some() {
        ctx.request_repaint_after(Duration::from_millis(300));
    }
    Ok(())
}

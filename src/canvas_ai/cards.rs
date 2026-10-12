//! Agent presentation is derived from bindings, never from a node's display text.
use super::*;

#[derive(Default)]
pub(super) struct State {
    avatar: Option<egui::TextureHandle>,
    drafts: HashMap<(uuid::Uuid, String), String>,
    active: Option<(uuid::Uuid, String)>,
    composing: bool,
}

struct Card {
    index: usize,
    workflow: String,
    task: String,
    title: String,
    role: String,
    status: String,
    detail: String,
    files: Vec<String>,
    can_send: bool,
    connected: bool,
}

fn selected_card<'a>(cards: &'a [Card], selected: &HashSet<usize>) -> Option<&'a Card> {
    if selected.len() != 1 {
        return None;
    }
    cards.iter().find(|card| selected.contains(&card.index))
}

fn cards(doc: &Document, bindings: &Bindings, owners: &HashMap<String, uuid::Uuid>) -> Vec<Card> {
    let mut cards = Vec::new();
    for binding in &bindings.workflows {
        let workflow = doc
            .history_archive()
            .workflows
            .iter()
            .find(|w| w.id == binding.workflow_id);
        for agent in &binding.agents {
            let Some(index) = doc.nodes.iter().position(|n| n.id == agent.group_id) else {
                continue;
            };
            let task = workflow
                .and_then(|w| w.plan.as_ref().or(w.draft_plan.as_ref()))
                .and_then(|p| p.tasks.iter().find(|t| t.id == agent.task_id));
            let run =
                workflow.and_then(|w| w.runs.iter().rev().find(|r| r.task_id == agent.task_id));
            use project_graph_bevy_canvas::project_history::ArchivedStatus as S;
            let connected = owners.get(&binding.workflow_id) == Some(&doc.instance_token());
            let status = run.map_or("等待依赖", |r| match r.status {
                S::Queued => "排队中",
                S::Running => "执行中",
                S::Stopping => "停止中",
                S::Succeeded => "已完成",
                S::Failed => "失败",
                S::Cancelled => "已取消",
                S::Unknown => "状态未知",
                S::StorageBlocked => "保存受阻",
            });
            cards.push(Card {
                index,
                workflow: binding.workflow_id.clone(),
                task: agent.task_id.clone(),
                title: task.map_or_else(
                    || {
                        if agent.task_id == "$plan" {
                            "规划 Agent".into()
                        } else {
                            "Skill Agent".into()
                        }
                    },
                    |t| t.title.clone(),
                ),
                role: task.map_or("任务拆解与协作".into(), |t| t.role.clone()),
                status: if connected {
                    status.into()
                } else {
                    format!("归档 · {status}")
                },
                detail: run
                    .and_then(|r| r.error.clone())
                    .or_else(|| task.map(|t| t.instruction.clone()))
                    .or_else(|| workflow.map(|w| w.goal.clone()))
                    .unwrap_or_default(),
                files: run
                    .and_then(|r| r.proposal.as_ref())
                    .map_or_else(Vec::new, |p| {
                        p.files.iter().take(2).map(|f| f.path.clone()).collect()
                    }),
                can_send: connected
                    && task.is_some()
                    && run.is_some_and(|r| r.status == S::Succeeded && r.proposal.is_some()),
                connected,
            });
        }
    }
    cards
}

fn screen_rect(
    rect: [f32; 4],
    camera: &Transform,
    scale: f32,
    window: &Window,
    zoom: f32,
) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(
            ((rect[0] - camera.translation.x) / scale + window.width() * 0.5) / zoom,
            ((rect[1] + camera.translation.y) / scale + window.height() * 0.5) / zoom,
        ),
        egui::vec2(rect[2], rect[3]) / (scale * zoom),
    )
}

fn text(
    painter: &egui::Painter,
    position: egui::Pos2,
    value: &str,
    size: f32,
    width: f32,
    rows: usize,
    color: egui::Color32,
) {
    let mut job = egui::text::LayoutJob::simple(
        value.into(),
        egui::FontId::proportional(size),
        color,
        width.max(1.0),
    );
    job.wrap.max_rows = rows;
    let galley = painter.layout_job(job);
    painter.galley(position, galley, color);
}

impl State {
    pub(super) fn handoff_canvas_press(&mut self, ctx: &egui::Context) -> bool {
        let Some(key) = &self.active else {
            return false;
        };
        let id = egui::Id::new(("agent-message", key));
        if !self.composing
            && ctx.memory(|m| m.has_focus(id))
            && ctx.input(|i| i.pointer.primary_pressed())
            && !ctx.is_pointer_over_egui()
        {
            ctx.memory_mut(|m| m.surrender_focus(id));
            return true;
        }
        false
    }

    pub(super) fn clear_sent(&mut self, token: uuid::Uuid, node: &str) {
        self.drafts.remove(&(token, node.into()));
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        doc: &mut Document,
        bindings: &Bindings,
        owners: &HashMap<String, uuid::Uuid>,
        selected: &HashSet<usize>,
        display: &sections::Display,
        camera: &Transform,
        projection: &OrthographicProjection,
        window: &Window,
        busy: bool,
        light: bool,
        instances: &HashSet<uuid::Uuid>,
    ) -> ai::Result<Option<(ai::Command, String)>> {
        self.drafts
            .retain(|(token, _), _| instances.contains(token));
        let cards = cards(doc, bindings, owners);
        if !cards.is_empty() && self.avatar.is_none() {
            let image = image::load_from_memory(include_bytes!(
                "../../assets/branding/app-icons/128x128.png"
            ))
            .map_err(|e| format!("Agent 头像加载失败：{e}"))?
            .to_rgba8();
            self.avatar = Some(ctx.load_texture(
                "agent-avatar",
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            ));
        }
        let p = theme::chrome_palette(light);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            "agent-cards".into(),
        ));
        for card in &cards {
            let Some(node) = sections::node(Some(display), doc, card.index) else {
                continue;
            };
            let mut rect = screen_rect(
                node.rect,
                camera,
                projection.scale,
                window,
                ctx.zoom_factor(),
            );
            if !rect.intersects(ctx.content_rect()) {
                continue;
            }
            let s = 1.0 / (projection.scale * ctx.zoom_factor());
            if !node.collapsed {
                rect.max.y = rect.min.y + 64.0 * s;
            }
            let painter = painter.with_clip_rect(rect.intersect(ctx.content_rect()));
            painter.rect_filled(rect, (18.0 * s).clamp(0.0, 255.0) as u8, p.surface);
            painter.rect_stroke(
                rect,
                (18.0 * s).clamp(0.0, 255.0) as u8,
                egui::Stroke::new(
                    if selected.contains(&card.index) {
                        1.8
                    } else {
                        1.0
                    },
                    if selected.contains(&card.index) {
                        p.active
                    } else {
                        p.border
                    },
                ),
                egui::StrokeKind::Inside,
            );
            let origin = rect.min + egui::vec2(16.0, 16.0) * s;
            if let Some(avatar) = &self.avatar {
                painter.image(
                    avatar.id(),
                    egui::Rect::from_min_size(origin, egui::vec2(48.0, 48.0) * s),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            text(
                &painter,
                origin + egui::vec2(60.0, 0.0) * s,
                &card.title,
                19.0 * s,
                rect.width() - 96.0 * s,
                1,
                p.foreground,
            );
            text(
                &painter,
                origin + egui::vec2(60.0, 29.0) * s,
                &card.status,
                13.0 * s,
                rect.width() - 96.0 * s,
                1,
                p.active,
            );
            if node.collapsed && s >= 0.35 {
                text(
                    &painter,
                    origin + egui::vec2(0.0, 65.0) * s,
                    &card.detail,
                    14.0 * s,
                    rect.width() - 32.0 * s,
                    2,
                    p.muted,
                );
                let footer = if card.files.is_empty() {
                    card.role.clone()
                } else {
                    card.files.join(" · ")
                };
                text(
                    &painter,
                    origin + egui::vec2(0.0, 132.0) * s,
                    &footer,
                    12.0 * s,
                    rect.width() - 32.0 * s,
                    1,
                    p.muted,
                );
            }
        }
        let selected = selected_card(&cards, selected);
        let target = selected.map(|c| (doc.instance_token(), doc.nodes[c.index].id.clone()));
        if self.active != target {
            if let Some(old) = &self.active {
                ctx.memory_mut(|m| m.surrender_focus(egui::Id::new(("agent-message", old))));
            }
            self.composing = false;
            self.active = target.clone();
        }
        let Some(card) = selected else {
            return Ok(None);
        };
        let Some(node) = sections::node(Some(display), doc, card.index) else {
            return Ok(None);
        };
        let rect = screen_rect(
            node.rect,
            camera,
            projection.scale,
            window,
            ctx.zoom_factor(),
        );
        if !rect.intersects(ctx.content_rect()) {
            return Ok(None);
        }
        let Some(key) = target else {
            return Ok(None);
        };
        let draft = self.drafts.entry(key.clone()).or_default();
        let viewport = ctx.content_rect();
        let width = 380.0_f32.min((viewport.width() - 32.0).max(120.0));
        let x = (rect.center().x - width / 2.0).clamp(
            viewport.left() + 8.0,
            (viewport.right() - width - 8.0).max(viewport.left() + 8.0),
        );
        let y = (rect.bottom() + 12.0).min((viewport.bottom() - 196.0).max(viewport.top() + 80.0));
        let mut send = false;
        let mut toggle = false;
        egui::Area::new("selected-agent-composer".into())
            .fixed_pos(egui::pos2(x, y))
            .movable(false)
            .show(ctx, |ui| {
                theme::tool_frame(light).show(ui, |ui| {
                    theme::tool_style(ui, light);
                    ui.set_width((width - 32.0).max(88.0));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!("发给 {}", card.title))
                                .size(13.0)
                                .color(p.muted),
                        );
                        if ui
                            .small_button(if node.collapsed {
                                "展开记录"
                            } else {
                                "收起记录"
                            })
                            .clicked()
                        {
                            toggle = true;
                        }
                    });
                    let composing =
                        ctx.input(|i| editor::composition_frame(&mut self.composing, &i.events));
                    let response = egui::ScrollArea::vertical()
                        .id_salt(("agent-draft-scroll", &key))
                        .max_height(72.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(draft)
                                    .interactive(!busy)
                                    .id(egui::Id::new(("agent-message", &key)))
                                    .hint_text("补充想法，继续这个 Agent 的工作…")
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(2)
                                    .return_key(Some(egui::KeyboardShortcut::new(
                                        egui::Modifiers::SHIFT,
                                        egui::Key::Enter,
                                    ))),
                            )
                        })
                        .inner;
                    let enter = response.has_focus()
                        && !composing
                        && ctx.input(|i| {
                            i.events.iter().any(|event| {
                                matches!(event,
                            egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. }
                                if !modifiers.shift && !modifiers.alt && !modifiers.command)
                            })
                        })
                        && ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(if !card.connected {
                            "归档 · 在协作中连接本机流程后可发送"
                        } else if !card.can_send {
                            "等待完成后继续对话"
                        } else {
                            "Enter 发送 · Shift+Enter 换行"
                        });
                        let enabled =
                            card.can_send && !busy && !draft.trim().is_empty() && !composing;
                        send = (ui
                            .add_enabled(enabled, egui::Button::new("发送 ↑"))
                            .clicked()
                            || enter)
                            && enabled;
                    });
                });
            });
        if toggle {
            doc.set_sections_collapsed(&HashSet::from([card.index]), !node.collapsed)
                .map_err(|e| e.to_string())?;
        }
        Ok(send.then(|| {
            (
                ai::Command::FollowUp {
                    id: card.workflow.clone(),
                    task_id: card.task.clone(),
                    message: draft.clone(),
                },
                key.1,
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_press_releases_only_the_agent_composer_focus() {
        let ctx = egui::Context::default();
        let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
        warmup.textures_delta.clear();
        let key = (uuid::Uuid::new_v4(), "agent".into());
        let id = egui::Id::new(("agent-message", &key));
        let mut state = State {
            active: Some(key),
            ..default()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(egui::pos2(900.0, 600.0)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(900.0, 600.0),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..default()
            },
            |_| {
                ctx.memory_mut(|m| m.request_focus(id));
                state.composing = true;
                assert!(!state.handoff_canvas_press(&ctx));
                state.composing = false;
                assert!(state.handoff_canvas_press(&ctx));
                assert!(!ctx.memory(|m| m.has_focus(id)));
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn rejected_send_keeps_draft_until_runtime_accepts_it() {
        let mut doc = Document::blank().unwrap();
        let mut tabs = tabs::Tabs::from_args(&doc, &[]).unwrap();
        let token = doc.instance_token();
        let mut state = super::super::State::default();
        state
            .cards
            .drafts
            .insert((token, "agent".into()), "保留这条消息".into());
        let (tx, rx) = mpsc::channel();
        state.receiver = Mutex::new(Some(rx));
        state.pending_message = Some((token, "agent".into()));
        tx.send((Err("保存失败".into()), None)).unwrap();
        assert!(state.poll(&mut doc, &mut tabs).is_err());
        assert_eq!(state.cards.drafts[&(token, "agent".into())], "保留这条消息");
        let (tx, rx) = mpsc::channel();
        state.receiver = Mutex::new(Some(rx));
        state.pending_message = Some((token, "agent".into()));
        tx.send((Ok(ai::Snapshot::default()), None)).unwrap();
        state.poll(&mut doc, &mut tabs).unwrap();
        assert!(state.cards.drafts.is_empty());
    }

    #[test]
    fn composer_requires_exactly_one_bound_agent_and_isolates_documents() {
        let mut doc = Document::blank().unwrap();
        let normal = doc.create_text_node([0.0, 0.0]).unwrap();
        doc.graph.nodes[normal].text = "Agent · 已完成".into();
        let log = doc.create_text_node([400.0, 0.0]).unwrap();
        let group = doc.create_group(&HashSet::from([log])).unwrap();
        let bindings = Bindings {
            workflows: vec![ai::canvas::Binding {
                workflow_id: "workflow".into(),
                input_ids: vec![],
                anchor: [0.0; 2],
                answer_maps: vec![],
                agents: vec![ai::canvas::Agent {
                    task_id: "task".into(),
                    group_id: doc.nodes[group].id.clone(),
                    log_id: doc.nodes[log].id.clone(),
                    presented_run_id: None,
                }],
            }],
        };
        bindings.write(&mut doc).unwrap();
        let views = cards(&doc, &bindings, &HashMap::new());
        for selection in [
            HashSet::new(),
            HashSet::from([normal]),
            HashSet::from([log]),
            HashSet::from([normal, group]),
        ] {
            assert!(selected_card(&views, &selection).is_none());
        }
        let card = selected_card(&views, &HashSet::from([group])).unwrap();
        assert!(
            !card.can_send,
            "archived nodes cannot acquire execution ownership"
        );
        let other = Document::blank().unwrap();
        let mut state = State::default();
        let id = doc.nodes[group].id.clone();
        state
            .drafts
            .insert((doc.instance_token(), id.clone()), "first".into());
        state
            .drafts
            .insert((other.instance_token(), id.clone()), "second".into());
        state.clear_sent(doc.instance_token(), &id);
        assert_eq!(state.drafts[&(other.instance_token(), id)], "second");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agents.mio");
        doc.save(&path).unwrap();
        let reopened = Document::open(&path).unwrap();
        assert_eq!(
            cards(
                &reopened,
                &Bindings::read(&reopened).unwrap(),
                &HashMap::new()
            )
            .len(),
            1
        );
    }
}

use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

fn task(id: &str, dependencies: &[&str], parent: Option<&str>) -> Task {
    Task {
        id: id.into(),
        title: id.into(),
        role: "代码 Agent".into(),
        instruction: format!("task-{id}"),
        acceptance: "返回有效代码并保留其他分支成果".into(),
        dependencies: dependencies.iter().map(|s| (*s).into()).collect(),
        parent_agent: parent.map(str::to_owned),
    }
}
fn plan() -> Plan {
    Plan {
        tasks: vec![
            task("code", &[], None),
            task("tests", &[], None),
            task("check", &["code"], Some("code")),
            task("join", &["check", "tests"], None),
        ],
    }
}

#[derive(Default)]
struct Fake {
    active: AtomicUsize,
    maximum: AtomicUsize,
    fail_once: AtomicBool,
    conflicting: bool,
    gate: Option<Arc<AtomicBool>>,
}
impl pi::Executor for Fake {
    fn execute(
        &self,
        _: &str,
        prompt: &str,
        _: &Path,
        cancel: &AtomicBool,
        _: u64,
        progress: &dyn Fn(&str),
    ) -> Result<pi::Output> {
        let n = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(n, Ordering::SeqCst);
        while self
            .gate
            .as_ref()
            .is_some_and(|gate| gate.load(Ordering::Acquire))
        {
            if cancel.load(Ordering::Acquire) {
                self.active.fetch_sub(1, Ordering::SeqCst);
                return Err("运行已取消".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..15 {
            thread::sleep(Duration::from_millis(5));
            if cancel.load(Ordering::Acquire) {
                self.active.fetch_sub(1, Ordering::SeqCst);
                return Err("运行已取消".into());
            }
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        if prompt.contains("task-code") && self.fail_once.swap(false, Ordering::SeqCst) {
            return Err("可控的执行器故障".into());
        }
        let text = if prompt.contains("项目 Skill") {
            serde_json::to_string(&Proposal { summary: "基于固定运行整理工作流".into(), files: vec![SourceFile { path: "SKILL.md".into(), content: "---\nname: code-workflow\ndescription: Use for reviewed parallel code tasks\n---\n# Code workflow\nInspect inputs, delegate, verify each run; failed and incomplete runs are not validated.\n".into() }] }).unwrap()
        } else if prompt.contains("代码协作规划") {
            serde_json::to_string(&plan()).unwrap()
        } else {
            let files = if self.conflicting
                && (prompt.contains("本任务：task-code\n")
                    || prompt.contains("本任务：task-tests\n"))
            {
                vec![SourceFile {
                    path: "shared.js".into(),
                    content: if prompt.contains("本任务：task-code\n") {
                        "// first"
                    } else {
                        "// second"
                    }
                    .into(),
                }]
            } else if prompt.contains("本任务：task-code\n") {
                vec![SourceFile {
                    path: "lib.js".into(),
                    content: "export const add = (a,b) => a+b;".into(),
                }]
            } else if prompt.contains("本任务：task-tests\n") {
                vec![SourceFile {
                    path: "lib.test.js".into(),
                    content: "// generated test".into(),
                }]
            } else {
                vec![]
            };
            serde_json::to_string(&Proposal {
                summary: "完成；未执行测试".into(),
                files,
            })
            .unwrap()
        };
        progress("正在生成代码");
        Ok(pi::Output {
            text,
            usage: None,
            session_id: Some("fake-session".into()),
        })
    }
}
fn create(runtime: &Runtime, project: &Path) -> String {
    fs::write(project.join("main.js"), "// original source").unwrap();
    runtime
        .command(Command::Create {
            project: project.to_string_lossy().into_owned(),
            paths: vec!["main.js".into()],
            goal: "为代码实现求和并增加测试".into(),
            model: "test/fake".into(),
            test_command: vec![],
        })
        .unwrap()
        .workflows
        .last()
        .unwrap()
        .id
        .clone()
}
fn wait(runtime: &Runtime, predicate: impl Fn(&Workflow) -> bool) -> Workflow {
    let start = Instant::now();
    loop {
        let w = runtime.snapshot().unwrap().workflows[0].clone();
        if predicate(&w) {
            return w;
        }
        assert!(start.elapsed() < Duration::from_secs(8), "timeout: {w:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[derive(Default)]
struct DelegatingFake {
    children: AtomicUsize,
    hold: AtomicBool,
    abandon: bool,
}
impl pi::Executor for DelegatingFake {
    fn execute(
        &self,
        model: &str,
        prompt: &str,
        cwd: &Path,
        cancel: &AtomicBool,
        timeout: u64,
        progress: &dyn Fn(&str),
    ) -> Result<pi::Output> {
        if prompt.contains("你是独立子 Agent") {
            let number = self.children.fetch_add(1, Ordering::SeqCst) + 1;
            let start = Instant::now();
            while self.children.load(Ordering::SeqCst) < 2 || self.hold.load(Ordering::Acquire) {
                if cancel.load(Ordering::Acquire) {
                    return Err("运行已取消".into());
                }
                if start.elapsed().as_secs() > 5 {
                    return Err("children did not run concurrently".into());
                }
                thread::yield_now();
            }
            return Ok(pi::Output {
                text: serde_json::to_string(&Proposal {
                    summary: "独立子任务成果".into(),
                    files: vec![SourceFile {
                        path: format!("child-{number}.txt"),
                        content: "saved child result".into(),
                    }],
                })
                .unwrap(),
                usage: None,
                session_id: Some(id()),
            });
        }
        Fake::default().execute(model, prompt, cwd, cancel, timeout, progress)
    }
    fn execute_delegating(
        &self,
        request: pi::Execution<'_>,
        tools: &dyn Fn(&str, &str, serde_json::Value) -> Result<serde_json::Value>,
    ) -> Result<pi::Output> {
        let args = serde_json::json!({"title":"独立检查","instruction":"分析一个独立方面","acceptance":"返回结论"});
        let first = tools("first", "spawn_agent", args.clone())?;
        assert_eq!(first, tools("first", "spawn_agent", args.clone())?);
        assert!(
            tools(
                "first",
                "spawn_agent",
                serde_json::json!({"title":"修改","instruction":"不同任务","acceptance":"结论"})
            )
            .is_err()
        );
        let second = tools("second", "spawn_agent", args)?;
        if self.abandon {
            return Ok(pi::Output {
                text: r#"{"summary":"premature completion","files":[]}"#.into(),
                usage: None,
                session_id: None,
            });
        }
        assert!(
            tools(
                "foreign",
                "wait_agent",
                serde_json::json!({"run_id":"not-my-child"})
            )
            .is_err()
        );
        for (call, child) in [("wait-first", first), ("wait-second", second)] {
            let result = tools(
                call,
                "wait_agent",
                serde_json::json!({"run_id":child["run_id"]}),
            )?;
            if request.cancel.load(Ordering::Acquire) {
                return Err("运行已取消".into());
            }
            assert_eq!(result["status"], "succeeded");
        }
        Ok(pi::Output {
            text: r#"{"summary":"已汇总两个固定成果","files":[]}"#.into(),
            usage: None,
            session_id: Some(id()),
        })
    }
}

fn start_delegating(runtime: &Runtime) -> String {
    let id = runtime
        .command(Command::Create {
            project: String::new(),
            paths: vec![],
            goal: "并行分析问题".into(),
            model: "test/fake".into(),
            test_command: vec![],
        })
        .unwrap()
        .workflows[0]
        .id
        .clone();
    wait(runtime, |w| w.draft_plan.is_some());
    runtime
        .command(Command::SetPlan {
            id: id.clone(),
            plan: Plan {
                tasks: vec![task("root", &[], None)],
            },
        })
        .unwrap();
    runtime.command(Command::Start { id: id.clone() }).unwrap();
    id
}

#[test]
fn dynamic_delegation_is_parallel_idempotent_and_survives_mio() {
    let directory = tempfile::tempdir().unwrap();
    let fake = Arc::new(DelegatingFake {
        hold: AtomicBool::new(true),
        ..Default::default()
    });
    let runtime = Runtime::with_executor(directory.path().into(), fake.clone()).unwrap();
    let workflow_id = start_delegating(&runtime);
    wait(&runtime, |w| {
        w.runs.iter().filter(|r| r.delegation.is_some()).count() == 2
    });
    let mut doc = crate::document::Document::blank().unwrap();
    let mut bindings = canvas::Bindings::default();
    bindings.workflows.push(canvas::Binding {
        workflow_id,
        input_ids: vec![],
        anchor: [0.0, 0.0],
        agents: vec![],
        answer_maps: vec![],
    });
    bindings
        .sync(&mut doc, &runtime.snapshot().unwrap())
        .unwrap();
    fake.hold.store(false, Ordering::Release);
    let w = wait(&runtime, |w| {
        latest(w, "root").is_some_and(|r| r.status == Status::Succeeded)
    });
    assert_eq!(fake.children.load(Ordering::SeqCst), 2);
    let parent = latest(&w, "root").unwrap();
    assert_eq!(parent.delegated_results.len(), 2);
    assert_eq!(runtime::effective_files(&w, &parent.id).unwrap().len(), 2);
    assert_eq!(w.runs.len(), 4);
    for child in w.runs.iter().filter(|r| r.delegation.is_some()) {
        assert_eq!(child.delegation.as_ref().unwrap().parent_run_id, parent.id);
        assert!(!child.sources.contains(&parent.id));
    }
    validate_plan(w.plan.as_ref().unwrap()).unwrap();
    bindings
        .sync(&mut doc, &runtime.snapshot().unwrap())
        .unwrap();
    assert_eq!(bindings.workflows[0].agents.len(), 4);
    assert!(doc.graph.nodes.iter().any(|n| n.text.contains("独立检查")));
    let path = directory.path().join("delegation.mio");
    doc.save(&path).unwrap();
    let reopened = crate::document::Document::open(&path).unwrap();
    assert_eq!(
        reopened.history_archive().workflows[0]
            .runs
            .iter()
            .filter(|r| r.delegation.is_some())
            .count(),
        2
    );
    runtime.shutdown().unwrap();
    drop(runtime);
    let reopened =
        Runtime::with_executor(directory.path().into(), Arc::new(Fake::default())).unwrap();
    assert_eq!(reopened.snapshot().unwrap().workflows[0].runs.len(), 4);
    reopened.shutdown().unwrap();
}

#[test]
fn stopping_parent_cancels_children_without_deleting_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let fake = Arc::new(DelegatingFake {
        hold: AtomicBool::new(true),
        ..Default::default()
    });
    let runtime = Runtime::with_executor(directory.path().into(), fake).unwrap();
    let workflow_id = start_delegating(&runtime);
    wait(&runtime, |w| {
        w.runs
            .iter()
            .filter(|r| r.delegation.is_some() && r.status == Status::Running)
            .count()
            == 2
    });
    runtime
        .command(Command::Stop {
            id: workflow_id,
            task_id: Some("root".into()),
        })
        .unwrap();
    let w = wait(&runtime, |w| {
        w.runs
            .iter()
            .filter(|r| r.task_id != "$plan")
            .all(|r| r.status == Status::Cancelled)
    });
    assert_eq!(w.runs.len(), 4);
    runtime.shutdown().unwrap();
}

#[test]
fn parent_cannot_finish_successfully_with_uncollected_children() {
    let directory = tempfile::tempdir().unwrap();
    let fake = Arc::new(DelegatingFake {
        hold: AtomicBool::new(true),
        abandon: true,
        ..Default::default()
    });
    let runtime = Runtime::with_executor(directory.path().into(), fake).unwrap();
    start_delegating(&runtime);
    let w = wait(&runtime, |w| {
        latest(w, "root").is_some_and(|r| r.status == Status::Failed)
            && w.runs
                .iter()
                .filter(|r| r.delegation.is_some())
                .all(|r| r.status == Status::Cancelled)
    });
    assert_eq!(w.runs.len(), 4);
    assert!(
        latest(&w, "root")
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("未收集")
    );
    runtime.shutdown().unwrap();
}

#[test]
#[ignore = "calls DeepSeek through the bundled Pi engine using configured credentials"]
fn real_pi_dynamic_delegation() {
    let directory = tempfile::tempdir().unwrap();
    // Seed an explicitly approved plan without spending a planning model call.
    let runtime =
        Runtime::with_executor(directory.path().into(), Arc::new(Fake::default())).unwrap();
    let workflow_id = runtime
        .command(Command::Create {
            project: String::new(),
            paths: vec![],
            goal: "比较 SQLite 的事务与索引".into(),
            model: "deepseek/deepseek-flash".into(),
            test_command: vec![],
        })
        .unwrap()
        .workflows[0]
        .id
        .clone();
    wait(&runtime, |w| w.draft_plan.is_some());
    let mut root = task("root", &[], None);
    root.instruction = "必须调用 spawn_agent 两次，分别让子 Agent 用一句话解释 SQLite 事务与索引。先创建两个，再逐一调用 wait_agent，最后汇总两个成果。不创建文件。".into();
    runtime
        .command(Command::SetPlan {
            id: workflow_id.clone(),
            plan: Plan { tasks: vec![root] },
        })
        .unwrap();
    runtime.shutdown().unwrap();
    drop(runtime);
    let runtime = Runtime::open(directory.path().into()).unwrap();
    runtime.command(Command::Start { id: workflow_id }).unwrap();
    let start = Instant::now();
    loop {
        let snapshot = runtime.snapshot().unwrap();
        let w = &snapshot.workflows[0];
        let root = latest(w, "root").unwrap();
        if !matches!(root.status, Status::Running | Status::Queued) {
            runtime.shutdown().unwrap();
            assert_eq!(root.status, Status::Succeeded, "{:?}", root.error);
            assert_eq!(w.runs.iter().filter(|r| r.delegation.is_some()).count(), 2);
            assert_eq!(root.delegated_results.len(), 2);
            assert!(w.runs.iter().all(|r| r.status == Status::Succeeded));
            let history =
                crate::project_history::History::capture(&snapshot, std::slice::from_ref(&w.id))
                    .unwrap();
            let mut document = crate::document::Document::blank().unwrap();
            document.attach_history(history.clone()).unwrap();
            let path = directory.path().join("real-delegation.mio");
            document.save(&path).unwrap();
            assert_eq!(
                crate::document::Document::open(&path)
                    .unwrap()
                    .history_archive(),
                &history
            );
            break;
        }
        if start.elapsed().as_secs() > 200 {
            runtime.shutdown().unwrap();
            panic!("real delegation timed out");
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn canvas_only_workflow_runs_and_survives_mio_without_external_project() {
    use crate::document::Document;
    let storage = tempfile::tempdir().unwrap();
    let runtime = Runtime::with_executor(storage.path().into(), Arc::new(Fake::default())).unwrap();
    let snapshot = runtime
        .command(Command::Create {
            project: String::new(),
            paths: vec![],
            goal: "讲解 Pi Agent".into(),
            model: "test/fake".into(),
            test_command: vec![],
        })
        .unwrap();
    let id = snapshot.workflows[0].id.clone();
    let planned = wait(&runtime, |w| w.draft_plan.is_some());
    assert!(planned.project.is_empty());
    assert!(planned.files.is_empty());
    runtime
        .command(Command::SetPlan {
            id: id.clone(),
            plan: planned.draft_plan.unwrap(),
        })
        .unwrap();
    runtime.command(Command::Start { id: id.clone() }).unwrap();
    wait(&runtime, |w| {
        w.runs.len() == 5 && w.runs.iter().all(|r| r.status == Status::Succeeded)
    });
    let mut doc = Document::blank().unwrap();
    let input = doc.create_text_node([0.0, 0.0]).unwrap();
    doc.graph.nodes[input].text = "讲解 Pi Agent".into();
    let mut bindings = canvas::Bindings::default();
    bindings.workflows.push(canvas::Binding {
        workflow_id: id,
        input_ids: vec![doc.nodes[input].id.clone()],
        anchor: [360.0, 0.0],
        agents: vec![],
        answer_maps: vec![],
    });
    let mut snapshot = runtime.snapshot().unwrap();
    for run in &mut snapshot.workflows[0].runs {
        if let Some(proposal) = &mut run.proposal {
            proposal.summary =
                "- 定义\n  - 一个代码助手\n- 使用\n  - 提交目标\n  - 查看结果".into();
        }
    }
    bindings.sync(&mut doc, &snapshot).unwrap();
    assert!(bindings.workflows[0].agents.is_empty());
    assert_eq!(bindings.workflows[0].answer_maps.len(), 4);
    let map = &bindings.workflows[0].answer_maps[0];
    assert_eq!(map.node_ids.len(), 5);
    let root = doc
        .nodes
        .iter()
        .position(|n| n.id == map.node_ids[0])
        .unwrap();
    let child = doc
        .nodes
        .iter()
        .position(|n| n.id == map.node_ids[1])
        .unwrap();
    assert!(doc.edges.iter().any(|e| e.members == [input, root]));
    assert!(doc.edges.iter().any(|e| e.members == [root, child]));
    assert!(doc.nodes.iter().all(|n| !n.text.contains("任务规划")));
    if let Some(path) = std::env::var_os("RAVOMI_MINDMAP_FIXTURE") {
        doc.save(Path::new(&path)).unwrap();
    }
    assert_eq!(
        bindings
            .selected_runs(&doc, &std::collections::HashSet::from([child]), &snapshot)
            .len(),
        1
    );
    doc.graph.nodes[child].text = "用户改写的内容".into();
    doc.delete_objects(
        &std::collections::HashSet::from([root]),
        &Default::default(),
    )
    .unwrap();
    let count = doc.nodes.len();
    bindings.sync(&mut doc, &snapshot).unwrap();
    assert_eq!(doc.nodes.len(), count);
    assert!(doc.nodes.iter().any(|n| n.text == "用户改写的内容"));
    let destination = storage.path().join("canvas.mio");
    doc.save(&destination).unwrap();
    drop(runtime);
    fs::remove_file(storage.path().join("workflows.json")).unwrap();
    let restored = Document::open(&destination).unwrap();
    assert_eq!(
        canvas::Bindings::read(&restored).unwrap().workflows[0]
            .answer_maps
            .len(),
        4
    );
    assert_eq!(restored.history_archive(), doc.history_archive());
    assert_eq!(restored.history_archive().workflows[0].runs.len(), 5);
    assert!(
        !restored.history_archive().workflows[0].runs[1]
            .output
            .is_empty()
    );
}

#[test]
fn all_ready_agents_start_without_task_concurrency_or_call_caps() {
    let storage = tempfile::tempdir().unwrap();
    let gate = Arc::new(AtomicBool::new(false));
    let fake = Arc::new(Fake {
        gate: Some(gate.clone()),
        ..Fake::default()
    });
    let runtime = Runtime::with_executor(storage.path().into(), fake.clone()).unwrap();
    runtime
        .command(Command::Create {
            project: String::new(),
            paths: vec![],
            goal: "并行任务".into(),
            model: "test/fake".into(),
            test_command: vec![],
        })
        .unwrap();
    let planned = wait(&runtime, |w| w.draft_plan.is_some());
    let tasks = (0..20)
        .map(|i| task(&format!("worker-{i}"), &[], None))
        .collect();
    runtime
        .command(Command::SetPlan {
            id: planned.id.clone(),
            plan: Plan { tasks },
        })
        .unwrap();
    gate.store(true, Ordering::Release);
    runtime.command(Command::Start { id: planned.id }).unwrap();
    let started = Instant::now();
    while fake.active.load(Ordering::SeqCst) < 20 && started.elapsed() < Duration::from_secs(8) {
        thread::sleep(Duration::from_millis(10));
    }
    let active = fake.active.load(Ordering::SeqCst);
    gate.store(false, Ordering::Release);
    assert_eq!(active, 20);
    wait(&runtime, |w| {
        w.runs.len() == 21 && w.runs.iter().all(|r| r.status == Status::Succeeded)
    });
}

#[test]
fn canvas_agents_capture_group_inputs_and_preserve_deleted_nodes_and_run_versions() {
    use crate::document::Document;
    use std::collections::HashSet;
    let directory = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let runtime =
        Runtime::with_executor(directory.path().into(), Arc::new(Fake::default())).unwrap();
    let id = create(&runtime, project.path());
    let planned = wait(&runtime, |w| w.draft_plan.is_some());
    runtime
        .command(Command::SetPlan {
            id: id.clone(),
            plan: planned.draft_plan.unwrap(),
        })
        .unwrap();
    runtime.command(Command::Start { id: id.clone() }).unwrap();
    wait(&runtime, |w| {
        w.runs.len() == 5 && w.runs.iter().all(|r| r.status == Status::Succeeded)
    });
    let mut doc = Document::blank().unwrap();
    let text = doc.create_text_node([0.0, 0.0]).unwrap();
    doc.graph.nodes[text].text = "实现求和".into();
    let group = doc.create_group(&HashSet::from([text])).unwrap();
    let mut bindings = canvas::Bindings::default();
    let (goal, ids, anchor) = bindings
        .capture(&doc, &HashSet::from([group, text]))
        .unwrap();
    assert_eq!(goal.matches("实现求和").count(), 1);
    bindings.workflows.push(canvas::Binding {
        workflow_id: id,
        input_ids: ids,
        anchor,
        agents: vec![],
        answer_maps: vec![],
    });
    let snapshot = runtime.snapshot().unwrap();
    bindings.sync(&mut doc, &snapshot).unwrap();
    assert_eq!(bindings.workflows[0].agents.len(), 5);
    assert_eq!(doc.nodes[text].text, "实现求和");
    let count = doc.nodes.len();
    bindings.sync(&mut doc, &snapshot).unwrap();
    assert_eq!(doc.nodes.len(), count);
    let planner = &bindings.workflows[0].agents[0];
    let index = doc
        .nodes
        .iter()
        .position(|n| n.id == planner.group_id)
        .unwrap();
    assert!(!doc.nodes[index].collapsed);
    let log = &planner.log_id;
    assert!(
        doc.nodes
            .iter()
            .find(|n| &n.id == log)
            .unwrap()
            .text
            .contains("完成条件")
    );
    for agent in &bindings.workflows[0].agents[1..] {
        let run = snapshot.workflows[0]
            .runs
            .iter()
            .rev()
            .find(|run| run.task_id == agent.task_id)
            .unwrap();
        assert_eq!(
            doc.nodes
                .iter()
                .find(|n| n.id == agent.log_id)
                .unwrap()
                .text,
            run.proposal.as_ref().unwrap().summary
        );
        assert!(run.prompt.contains("默认用中文回答，借鉴 ASD-STE100"));
    }
    doc.set_sections_collapsed(&HashSet::from([index]), true)
        .unwrap();
    bindings.sync(&mut doc, &snapshot).unwrap();
    assert!(
        doc.nodes[index].collapsed,
        "refresh must preserve manual collapse"
    );
    assert!(bindings.capture(&doc, &HashSet::from([index])).is_err());
    let refs = bindings.selected_runs(&doc, &HashSet::from([index]), &snapshot);
    assert_eq!(refs[0].run_id, snapshot.workflows[0].runs[0].id);
    let file = directory.path().join("agents.mio");
    doc.save(&file).unwrap();
    let saved_revision = doc.revision;
    bindings.sync(&mut doc, &snapshot).unwrap();
    assert!(!doc.dirty());
    assert_eq!(doc.revision, saved_revision);
    let restored = Document::open(&file).unwrap();
    assert_eq!(restored.history_archive(), doc.history_archive());
    let mut reopened = Document::open(&file).unwrap();
    canvas::Bindings::read(&reopened)
        .unwrap()
        .sync(&mut reopened, &snapshot)
        .unwrap();
    assert!(
        reopened
            .nodes
            .iter()
            .find(|node| node.id == doc.nodes[index].id)
            .unwrap()
            .collapsed
    );
    let mut long_snapshot = snapshot.clone();
    let mut run = long_snapshot.workflows[0]
        .runs
        .iter()
        .find(|run| run.proposal.is_some())
        .unwrap()
        .clone();
    run.id = super::id();
    let expected = "这是完整的回答。\n".repeat(500);
    run.proposal.as_mut().unwrap().summary = expected.clone();
    let agent = bindings.workflows[0]
        .agents
        .iter()
        .find(|a| a.task_id == run.task_id)
        .unwrap();
    let log_id = agent.log_id.clone();
    long_snapshot.workflows[0].runs.push(run);
    bindings
        .clone()
        .sync(&mut reopened, &long_snapshot)
        .unwrap();
    assert_eq!(
        reopened.nodes.iter().find(|n| n.id == log_id).unwrap().text,
        expected
    );
    if let Some(path) = std::env::var_os("RAVOMI_CANVAS_AI_FIXTURE") {
        doc.save(Path::new(&path)).unwrap();
        fs::copy(
            directory.path().join("workflows.json"),
            Path::new(&path).with_extension("json"),
        )
        .unwrap();
    }
    assert_eq!(
        canvas::Bindings::read(&restored).unwrap().workflows[0]
            .agents
            .len(),
        5
    );
    doc.delete_objects(&HashSet::from([index]), &HashSet::new())
        .unwrap();
    let count = doc.nodes.len();
    bindings.sync(&mut doc, &snapshot).unwrap();
    assert_eq!(doc.nodes.len(), count);
}

#[test]
fn skill_generation_freezes_evidence_and_saves_only_new_project_skill() {
    let directory = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let runtime =
        Runtime::with_executor(directory.path().into(), Arc::new(Fake::default())).unwrap();
    let id = create(&runtime, project.path());
    let planned = wait(&runtime, |w| w.draft_plan.is_some());
    let sources = vec![skills::RunRef {
        workflow_id: id,
        run_id: planned.runs[0].id.clone(),
    }];
    let snapshot = runtime
        .command(Command::CreateSkill {
            project: project.path().display().to_string(),
            model: "test/fake".into(),
            name: "code-workflow".into(),
            sources: sources.clone(),
        })
        .unwrap();
    let skill_id = snapshot.workflows[1].id.clone();
    let start = Instant::now();
    let skill = loop {
        let w = runtime.snapshot().unwrap().workflows[1].clone();
        if w.runs[0].status == Status::Succeeded {
            break w;
        }
        assert!(start.elapsed() < Duration::from_secs(8), "{:?}", w.runs[0]);
        thread::sleep(Duration::from_millis(10));
    };
    assert!(skill.files[0].content.contains(&sources[0].run_id));
    assert!(!skill.files[0].content.contains("original source"));
    let content = &skill.runs[0].proposal.as_ref().unwrap().files[0].content;
    let destination = runtime
        .save_skill(&skill_id, &skill.runs[0].id, content)
        .unwrap();
    assert_eq!(
        destination,
        project.path().join(".agents/skills/code-workflow")
    );
    assert_eq!(
        fs::read_to_string(destination.join("SKILL.md")).unwrap(),
        *content
    );
    assert!(
        destination
            .join("references/ravomi-provenance.json")
            .is_file()
    );
    assert!(
        runtime
            .save_skill(&skill_id, &skill.runs[0].id, content)
            .is_err()
    );
    for name in ["../escape", "/global", "bad name", "", "UPPER"] {
        assert!(skills::validate(name, content).is_err());
    }
    assert!(skills::validate("code-workflow", "---\nname: code-workflow\n---\nBody").is_err());
    #[cfg(unix)]
    {
        fs::remove_dir_all(project.path().join(".agents")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), project.path().join(".agents")).unwrap();
        assert!(
            runtime
                .save_skill(&skill_id, &skill.runs[0].id, content)
                .is_err()
        );
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}

#[test]
fn skill_evidence_keeps_failed_running_states_and_rejects_missing_versions() {
    let directory = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let runtime =
        Runtime::with_executor(directory.path().into(), Arc::new(Fake::default())).unwrap();
    create(&runtime, project.path());
    let mut snapshot = runtime.snapshot().unwrap();
    snapshot.workflows[0].runs[0].status = Status::Failed;
    let first = snapshot.workflows[0].runs[0].clone();
    let mut second = first.clone();
    second.id = "in-flight".into();
    second.status = Status::Running;
    snapshot.workflows[0].runs.push(second);
    let sources = vec![skills::RunRef {
        workflow_id: snapshot.workflows[0].id.clone(),
        run_id: first.id,
    }];
    let generated = skills::workflow(
        &snapshot,
        &project.path().display().to_string(),
        "test/fake".into(),
        "code-workflow".into(),
        sources,
    )
    .unwrap();
    assert!(generated.files[0].content.contains("failed"));
    assert!(!generated.files[0].content.contains("in-flight"));
    assert!(
        skills::workflow(
            &snapshot,
            &project.path().display().to_string(),
            "test/fake".into(),
            "code-workflow".into(),
            vec![skills::RunRef {
                workflow_id: snapshot.workflows[0].id.clone(),
                run_id: "missing".into()
            }]
        )
        .is_err()
    );
    runtime.shutdown().unwrap();
}

#[test]
fn plan_separates_delegation_from_dependencies_and_rejects_cycles() {
    validate_plan(&plan()).unwrap();
    let mut value = plan();
    value.tasks[0].dependencies = vec!["join".into()];
    assert!(validate_plan(&value).unwrap_err().contains('环'));
    let mut value = plan();
    value.tasks[2].parent_agent = Some("tests".into());
    validate_plan(&value).unwrap();
    value.tasks[1].parent_agent = Some("check".into());
    assert!(validate_plan(&value).is_err());
    let mut value = plan();
    value.tasks[0].dependencies = vec!["missing".into()];
    assert!(validate_plan(&value).is_err());
}

#[test]
fn snapshots_reject_traversal_duplicate_names_and_binary_content() {
    for path in [
        "../outside",
        "/absolute",
        ".env",
        "nested/../x",
        "a\\b",
        "C:/x",
        "node_modules/x",
    ] {
        assert!(valid_path(path).is_err(), "{path}");
    }
    assert!(
        validate_files(&[SourceFile {
            path: "a.rs".into(),
            content: "\0".into()
        }])
        .is_err()
    );
    assert!(
        validate_files(&[
            SourceFile {
                path: "A.rs".into(),
                content: "".into()
            },
            SourceFile {
                path: "a.rs".into(),
                content: "".into()
            }
        ])
        .is_err()
    );
    assert!(
        validate_files(&[
            SourceFile {
                path: "src".into(),
                content: "".into()
            },
            SourceFile {
                path: "src/a.rs".into(),
                content: "".into()
            }
        ])
        .is_err()
    );
}

#[test]
fn parallel_handoff_review_export_and_reopen_keep_fixed_artifacts() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let export = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake::default());
    let runtime = Runtime::with_executor(storage.path().into(), fake.clone()).unwrap();
    let id = create(&runtime, project.path());
    wait(&runtime, |w| w.draft_plan.is_some());
    runtime
        .command(Command::SetPlan {
            id: id.clone(),
            plan: plan(),
        })
        .unwrap();
    runtime.command(Command::Start { id: id.clone() }).unwrap();
    // Idempotent start cannot dispatch the same task twice.
    runtime.command(Command::Start { id: id.clone() }).unwrap();
    let w = wait(&runtime, |w| {
        latest(w, "join").is_some_and(|r| r.status == Status::Succeeded)
    });
    assert_eq!(fake.maximum.load(Ordering::SeqCst), 2);
    assert_eq!(w.runs.len(), 5);
    let join = latest(&w, "join").unwrap();
    let files = runtime::effective_files(&w, &join.id).unwrap();
    assert!(files.iter().any(|f| f.path == "lib.js"));
    assert!(files.iter().any(|f| f.path == "lib.test.js"));
    assert_eq!(join.sources.len(), 2);
    assert!(runtime.export(&id, &join.id, export.path()).is_err());
    runtime
        .command(Command::Review {
            id: id.clone(),
            run_id: join.id.clone(),
            decision: "approved".into(),
        })
        .unwrap();
    let path = runtime.export(&id, &join.id, export.path()).unwrap();
    assert!(Path::new(&path).join("lib.js").is_file());
    assert!(!project.path().join("lib.js").exists());
    fs::write(project.path().join("main.js"), "// external change").unwrap();
    assert!(
        runtime
            .export(&id, &join.id, export.path())
            .unwrap_err()
            .contains("已变化")
    );
    runtime.shutdown().unwrap();
    drop(runtime);
    let reopened =
        Runtime::with_executor(storage.path().into(), Arc::new(Fake::default())).unwrap();
    assert_eq!(reopened.snapshot().unwrap().workflows[0].runs.len(), 5);
    assert_eq!(
        reopened.snapshot().unwrap().workflows[0].files[0].content,
        "// original source"
    );
}

#[test]
fn failure_blocks_dependents_and_retry_keeps_input_and_other_branch() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake {
        fail_once: AtomicBool::new(true),
        ..Fake::default()
    });
    let runtime = Runtime::with_executor(storage.path().into(), fake).unwrap();
    let id = create(&runtime, project.path());
    wait(&runtime, |w| w.draft_plan.is_some());
    runtime
        .command(Command::SetPlan {
            id: id.clone(),
            plan: plan(),
        })
        .unwrap();
    runtime.command(Command::Start { id: id.clone() }).unwrap();
    let w = wait(&runtime, |w| {
        latest(w, "code").is_some_and(|r| r.status == Status::Failed)
            && latest(w, "tests").is_some_and(|r| r.status == Status::Succeeded)
    });
    assert!(latest(&w, "join").is_none());
    let failed = latest(&w, "code").unwrap().clone();
    fs::write(project.path().join("main.js"), "// later edit").unwrap();
    runtime
        .command(Command::Retry {
            id,
            run_id: failed.id.clone(),
        })
        .unwrap();
    let w = wait(&runtime, |w| {
        latest(w, "join").is_some_and(|r| r.status == Status::Succeeded)
    });
    let retry = latest(&w, "code").unwrap();
    assert_eq!(retry.prompt, failed.prompt);
    assert_eq!(retry.input_files, failed.input_files);
    assert_eq!(retry.retry_of.as_deref(), Some(failed.id.as_str()));
    assert_eq!(w.runs.iter().filter(|r| r.task_id == "tests").count(), 1);
}

#[test]
fn cancellation_waits_for_executor_and_never_becomes_success() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let runtime = Runtime::with_executor(storage.path().into(), Arc::new(Fake::default())).unwrap();
    let id = create(&runtime, project.path());
    runtime
        .command(Command::Stop { id, task_id: None })
        .unwrap();
    wait(&runtime, |w| w.runs[0].status == Status::Cancelled);
    assert!(
        runtime.snapshot().unwrap().workflows[0]
            .draft_plan
            .is_none()
    );
}

#[test]
fn unknown_runs_do_not_restart_after_recovery() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let runtime = Runtime::with_executor(storage.path().into(), Arc::new(Fake::default())).unwrap();
    create(&runtime, project.path());
    runtime.shutdown().unwrap();
    assert!(Runtime::with_executor(storage.path().into(), Arc::new(Fake::default())).is_err());
    let mut snapshot = runtime.snapshot().unwrap();
    drop(runtime);
    snapshot.workflows[0].runs[0].status = Status::Running;
    fs::write(
        storage.path().join("workflows.json"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();
    let reopened =
        Runtime::with_executor(storage.path().into(), Arc::new(Fake::default())).unwrap();
    assert_eq!(
        reopened.snapshot().unwrap().workflows[0].runs[0].status,
        Status::Unknown
    );
}

#[cfg(unix)]
#[test]
fn symlink_inputs_are_not_followed() {
    let project = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/etc/passwd", project.path().join("source.rs")).unwrap();
    assert!(scan_project(project.path()).unwrap().is_empty());
    assert!(read_sources(project.path().to_str().unwrap(), &["source.rs".into()]).is_err());
}

#[test]
fn failed_storage_keeps_result_until_flush_without_reexecuting() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let runtime = Runtime::with_executor(storage.path().into(), Arc::new(Fake::default())).unwrap();
    create(&runtime, project.path());
    let record = storage.path().join("workflows.json");
    fs::remove_file(&record).unwrap();
    fs::create_dir(&record).unwrap(); // deterministic rename failure, also under root
    wait(&runtime, |w| w.runs[0].status == Status::StorageBlocked);
    assert!(runtime.snapshot().unwrap().storage_error.is_some());
    fs::remove_dir(&record).unwrap();
    let snapshot = runtime.command(Command::Flush).unwrap();
    assert!(snapshot.storage_error.is_none());
    assert_eq!(snapshot.workflows[0].runs.len(), 1);
    assert_eq!(snapshot.workflows[0].runs[0].status, Status::Succeeded);
    assert!(snapshot.workflows[0].draft_plan.is_some());
}

#[test]
fn corrupt_storage_is_preserved_and_not_replaced() {
    let storage = tempfile::tempdir().unwrap();
    let record = storage.path().join("workflows.json");
    fs::write(&record, "{broken").unwrap();
    assert!(Runtime::with_executor(storage.path().into(), Arc::new(Fake::default())).is_err());
    assert_eq!(fs::read_to_string(&record).unwrap(), "{broken");
}

#[test]
fn pause_keeps_running_agents_and_blocks_downstream_until_resume() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let gate = Arc::new(AtomicBool::new(false));
    let runtime = Runtime::with_executor(
        storage.path().into(),
        Arc::new(Fake {
            gate: Some(gate.clone()),
            ..Fake::default()
        }),
    )
    .unwrap();
    let id = create(&runtime, project.path());
    wait(&runtime, |w| w.draft_plan.is_some());
    runtime
        .command(Command::SetPlan {
            id: id.clone(),
            plan: plan(),
        })
        .unwrap();
    gate.store(true, Ordering::Release);
    runtime.command(Command::Start { id: id.clone() }).unwrap();
    runtime.command(Command::Pause { id: id.clone() }).unwrap();
    gate.store(false, Ordering::Release);
    let paused = wait(&runtime, |w| {
        w.runs.iter().all(|r| r.status == Status::Succeeded)
    });
    assert!(paused.paused);
    assert_eq!(paused.runs.len(), 3);
    runtime.command(Command::Start { id }).unwrap();
    wait(&runtime, |w| {
        w.runs.len() == 5 && w.runs.iter().all(|r| r.status == Status::Succeeded)
    });
    runtime.shutdown().unwrap();
}

#[test]
fn join_cannot_silently_drop_conflicting_branch_versions() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let runtime = Runtime::with_executor(
        storage.path().into(),
        Arc::new(Fake {
            conflicting: true,
            ..Fake::default()
        }),
    )
    .unwrap();
    let id = create(&runtime, project.path());
    wait(&runtime, |w| w.draft_plan.is_some());
    runtime
        .command(Command::SetPlan {
            id: id.clone(),
            plan: plan(),
        })
        .unwrap();
    runtime.command(Command::Start { id }).unwrap();
    let w = wait(&runtime, |w| {
        latest(w, "join").is_some_and(|r| r.status == Status::Failed)
    });
    assert!(
        latest(&w, "join")
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("冲突尚未解决")
    );
}

#[cfg(unix)]
#[test]
fn explicit_test_process_reports_failure_and_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let command = vec!["sh".into(), "-c".into(), "printf diagnostic; exit 7".into()];
    let error = pi::test_command(&command, directory.path(), &cancel, 5).unwrap_err();
    assert!(error.contains("diagnostic"), "{error}");
    cancel.store(true, Ordering::Release);
    let command = vec!["sh".into(), "-c".into(), "sleep 30".into()];
    let start = Instant::now();
    assert_eq!(
        pi::test_command(&command, directory.path(), &cancel, 5).unwrap_err(),
        "运行已取消"
    );
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
#[ignore = "Explicit live test: invokes the bundled Pi model with configured credentials"]
fn live_pi_code_workflow() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("math.cjs"), "module.exports = {};\n").unwrap();
    let runtime = Runtime::open(storage.path().into()).unwrap();
    let w = runtime.command(Command::Create { project: project.path().to_string_lossy().into_owned(), paths: vec!["math.cjs".into()], goal: "实现 CommonJS 的 add(a,b) 与 multiply(a,b)，两个独立分支分别实现；子 Agent 检查 add；最后汇总到 math.cjs 并新增 math.test.cjs，用 node:test 与 node:assert/strict 验证两个函数，不引入依赖。只写这两个文件。".into(), model: "deepseek/deepseek-flash".into(), test_command: vec!["node".into(), "--test".into()] }).unwrap().workflows.remove(0);
    let deadline = Instant::now();
    loop {
        let current = runtime.snapshot().unwrap().workflows.remove(0);
        assert!(deadline.elapsed() < Duration::from_secs(900));
        if let Some(plan) = current.draft_plan {
            fn depends(plan: &Plan, task: &Task, id: &str) -> bool {
                task.dependencies.iter().any(|d| {
                    d == id || depends(plan, plan.tasks.iter().find(|t| &t.id == d).unwrap(), id)
                })
            }
            assert!(plan.tasks.iter().any(|a| {
                plan.tasks
                    .iter()
                    .any(|b| a.id != b.id && !depends(&plan, a, &b.id) && !depends(&plan, b, &a.id))
            }));
            assert!(plan.tasks.iter().any(|t| t.parent_agent.is_some()));
            assert!(plan.tasks.iter().any(|t| t.dependencies.len() >= 2));
            runtime
                .command(Command::SetPlan {
                    id: w.id.clone(),
                    plan,
                })
                .unwrap();
            break;
        }
        if let Some(run) = current.runs.last() {
            assert!(run.status != Status::Failed, "{:?}", run.error);
        }
        thread::sleep(Duration::from_millis(200));
    }
    runtime
        .command(Command::Start { id: w.id.clone() })
        .unwrap();
    loop {
        let current = runtime.snapshot().unwrap().workflows.remove(0);
        assert!(deadline.elapsed() < Duration::from_secs(900));
        for run in &current.runs {
            assert!(run.status != Status::Failed, "{:?}", run.error);
        }
        if current.runs.len() == current.plan.as_ref().unwrap().tasks.len() + 1
            && current.runs.iter().all(|r| r.status == Status::Succeeded)
        {
            let plan = current.plan.as_ref().unwrap();
            let final_task = plan
                .tasks
                .iter()
                .find(|t| {
                    !plan
                        .tasks
                        .iter()
                        .any(|other| other.dependencies.contains(&t.id))
                })
                .unwrap();
            let result = latest(&current, &final_task.id).unwrap();
            runtime
                .command(Command::Test {
                    id: w.id.clone(),
                    run_id: result.id.clone(),
                })
                .unwrap();
            runtime
                .command(Command::Review {
                    id: w.id.clone(),
                    run_id: result.id.clone(),
                    decision: "approved".into(),
                })
                .unwrap();
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    let end = wait(&runtime, |w| {
        latest(w, "$test").is_some_and(|r| matches!(r.status, Status::Succeeded | Status::Failed))
    });
    let test = latest(&end, "$test").unwrap();
    assert_eq!(test.status, Status::Succeeded, "{:?}", test.error);
    println!(
        "Live Pi workflow: {} tasks; {} saved runs; tests passed",
        end.plan.unwrap().tasks.len(),
        end.runs.len()
    );
}

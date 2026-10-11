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
        let text = if prompt.contains("代码协作规划") {
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

#[test]
fn plan_rejects_cycles_unknown_inputs_and_unrelated_delegation() {
    validate_plan(&plan()).unwrap();
    let mut value = plan();
    value.tasks[0].dependencies = vec!["join".into()];
    assert!(validate_plan(&value).unwrap_err().contains('环'));
    let mut value = plan();
    value.tasks[2].parent_agent = Some("tests".into());
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
    let mut snapshot = runtime.snapshot().unwrap();
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
fn pause_revokes_queued_explicit_dispatch_until_resume() {
    let storage = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let gate = Arc::new(AtomicBool::new(true));
    let runtime = Runtime::with_executor(
        storage.path().into(),
        Arc::new(Fake {
            gate: Some(gate.clone()),
            ..Fake::default()
        }),
    )
    .unwrap();
    create(&runtime, project.path());
    create(&runtime, project.path());
    let id = create(&runtime, project.path());
    runtime
        .command(Command::SetPlan {
            id: id.clone(),
            plan: plan(),
        })
        .unwrap();
    runtime.command(Command::Pause { id: id.clone() }).unwrap();
    gate.store(false, Ordering::Release);
    wait(&runtime, |_| {
        runtime.snapshot().unwrap().workflows[..2]
            .iter()
            .all(|w| w.runs[0].status == Status::Succeeded)
    });
    assert_eq!(
        runtime.snapshot().unwrap().workflows[2].runs[0].status,
        Status::Queued
    );
    runtime.command(Command::Start { id }).unwrap();
    wait(&runtime, |_| {
        runtime.snapshot().unwrap().workflows[2]
            .draft_plan
            .is_some()
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

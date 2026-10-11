//! Fixed run evidence and project-local, no-clobber Skill delivery.
use super::*;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunRef {
    pub workflow_id: String,
    pub run_id: String,
}

pub fn validate(name: &str, content: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.ends_with('-')
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err("Skill 名称须为 1–64 位小写字母、数字或连字符".into());
    }
    if content.len() > 64 * 1024 || content.contains('\0') {
        return Err("Skill 内容超过 64 KiB 或包含无效字符".into());
    }
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return Err("Skill 缺少 YAML frontmatter".into());
    }
    let mut found_name = false;
    let mut description = false;
    for line in lines.by_ref() {
        if line == "---" {
            return if found_name && description && lines.any(|l| !l.trim().is_empty()) {
                Ok(())
            } else {
                Err("Skill 需要匹配目录的 name、单行 description 和正文".into())
            };
        }
        if let Some(value) = line.strip_prefix("name:") {
            found_name = value.trim().trim_matches(['\'', '"']) == name;
        }
        if let Some(value) = line.strip_prefix("description:") {
            description = !value.trim().trim_matches(['\'', '"']).is_empty()
                && !matches!(value.trim(), "|" | ">");
        }
    }
    Err("Skill frontmatter 未闭合".into())
}

pub(super) fn workflow(
    snapshot: &Snapshot,
    project: &str,
    model: String,
    name: String,
    sources: Vec<RunRef>,
) -> Result<Workflow> {
    validate(
        &name,
        &format!("---\nname: {name}\ndescription: workflow\n---\nDraft"),
    )?;
    if sources.is_empty()
        || sources.len() > 64
        || snapshot.workflows.len() >= 30
        || model.trim().is_empty()
        || model.len() > 200
    {
        return Err("请选择 1–64 个运行，填写模型；最多保留 30 个流程".into());
    }
    let project = Path::new(project)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !project.is_dir() {
        return Err("请选择代码项目目录".into());
    }
    let mut evidence = vec![];
    let mut seen = BTreeSet::new();
    for source in sources {
        if !seen.insert((source.workflow_id.clone(), source.run_id.clone())) {
            continue;
        }
        let w = snapshot
            .workflows
            .iter()
            .find(|w| w.id == source.workflow_id)
            .ok_or("来源流程不存在")?;
        let r = w
            .runs
            .iter()
            .find(|r| r.id == source.run_id)
            .ok_or("来源运行不存在")?;
        let task = w
            .plan
            .as_ref()
            .and_then(|p| p.tasks.iter().find(|t| t.id == r.task_id));
        // Raw prompts, source code, logs, credentials and absolute paths are deliberately excluded.
        evidence.push(serde_json::json!({
            "source": source, "workflowGoal": w.goal, "taskId": r.task_id, "status": r.status,
            "createdAt": r.created_at, "finishedAt": r.finished_at,
            "title": task.map(|t| &t.title), "role": task.map(|t| &t.role),
            "instruction": task.map(|t| &t.instruction), "acceptance": task.map(|t| &t.acceptance),
            "dependencies": task.map(|t| &t.dependencies), "parentAgent": task.and_then(|t| t.parent_agent.as_ref()),
            "sourceRuns": r.sources, "review": r.review, "retryOf": r.retry_of,
            "summary": r.proposal.as_ref().map(|p| &p.summary),
            "artifactPaths": r.proposal.as_ref().map(|p| p.files.iter().map(|f| &f.path).collect::<Vec<_>>()),
            "failure": if r.error.is_some() { Some("运行失败/中断；详情请在原运行核对") } else { None }
        }));
    }
    let mut content = serde_json::to_string_pretty(&evidence).map_err(|e| e.to_string())?;
    for w in &snapshot.workflows {
        content = content.replace(&w.project, "${PROJECT}");
    }
    // Do not forward common credential forms even if a user put them in a task title.
    content = content
        .split_inclusive(char::is_whitespace)
        .map(|word| {
            if word.contains("sk-") || word.to_ascii_lowercase().contains("api_key=") {
                "[REDACTED] "
            } else {
                word
            }
        })
        .collect();
    if content.len() > MAX_BYTES {
        return Err("所选运行证据过大，请缩小选择范围".into());
    }
    Ok(Workflow {
        id: id(),
        goal: name,
        project: project.to_string_lossy().into_owned(),
        model,
        files: vec![SourceFile {
            path: "workflow-evidence.json".into(),
            content,
        }],
        test_command: vec![],
        plan: None,
        draft_plan: None,
        runs: vec![],
        paused: true,
        max_calls: 4,
        timeout_seconds: 180,
    })
}

impl Runtime {
    /// Explicitly save the human-edited draft. Never replaces an installed skill.
    pub fn save_skill(&self, workflow_id: &str, run_id: &str, content: &str) -> Result<PathBuf> {
        let snapshot = self.snapshot()?;
        let w = snapshot
            .workflows
            .iter()
            .find(|w| w.id == workflow_id)
            .ok_or("流程不存在")?;
        let run = w
            .runs
            .iter()
            .find(|r| r.id == run_id && r.task_id == "$skill" && r.status == Status::Succeeded)
            .ok_or("Skill 草稿尚未成功生成")?;
        validate(&w.goal, content)?;
        let project = Path::new(&w.project)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let mut directory = project.clone();
        for part in [".agents", "skills"] {
            directory.push(part);
            match fs::symlink_metadata(&directory) {
                Ok(meta) if meta.is_symlink() || !meta.is_dir() => {
                    return Err("Skill 目录不能是符号链接或普通文件".into());
                }
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&directory).map_err(|e| e.to_string())?
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        let destination = directory.join(&w.goal);
        if destination.exists() {
            return Err("同名 Skill 已存在，请修改名称重新生成；不会覆盖".into());
        }
        let staging = tempfile::tempdir_in(&directory).map_err(|e| e.to_string())?;
        let mut file =
            fs::File::create(staging.path().join("SKILL.md")).map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::create_dir(staging.path().join("references")).map_err(|e| e.to_string())?;
        fs::write(staging.path().join("references/ravomi-provenance.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "generatorRun": run.id, "workflow": w.id, "evidence": w.files,
            "notice": "Human-reviewed generated workflow. Original project and third-party licenses remain applicable."
        })).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        // Claim the destination exclusively before publishing; no rename over an existing directory.
        fs::create_dir(&destination).map_err(|e| format!("无法创建 Skill（可能已存在）：{e}"))?;
        let result = (|| {
            fs::rename(
                staging.path().join("references"),
                destination.join("references"),
            )
            .map_err(|e| e.to_string())?;
            fs::rename(
                staging.path().join("SKILL.md"),
                destination.join("SKILL.md"),
            )
            .map_err(|e| e.to_string())?;
            Ok(destination.clone())
        })();
        if result.is_err() {
            fs::remove_dir_all(&destination)
                .map_err(|e| format!("保存失败，清理临时 Skill 失败：{e}"))?;
        }
        result
    }
}

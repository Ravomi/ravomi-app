use project_graph_bevy_canvas::collaboration::{self as ai, Command, Runtime, Snapshot, Status};
use project_graph_bevy_canvas::{project_history::History, session};
use std::path::Path;
use tauri::Manager;

pub struct Collaboration(pub Result<Runtime, String>);
#[tauri::command]
pub fn ai_configure(
    state: tauri::State<'_, Collaboration>,
    provider: String,
    api_key: String,
) -> Result<(), String> {
    runtime(&state)?.configure(&provider, api_key)
}
pub fn install(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Runtime::open(
        app.path()
            .app_data_dir()?
            .join(project_graph_bevy_canvas::storage::COLLABORATION_DIR_NAME),
    );
    app.manage(Collaboration(runtime));
    Ok(())
}
fn runtime(state: &Collaboration) -> Result<Runtime, String> {
    state.0.clone()
}

#[tauri::command]
pub fn ai_snapshot(state: tauri::State<'_, Collaboration>) -> Result<Snapshot, String> {
    runtime(&state)?.snapshot()
}

#[tauri::command]
pub async fn archive_project_workflow(
    app: tauri::AppHandle,
    project_id: String,
    document_token: String,
    workflow_id: String,
) -> Result<session::Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::AppState>();
        {
            let session = state
                .0
                .lock()
                .map_err(|_| "文档会话无法访问，请重新启动应用")?;
            check_project(&session, &project_id, &document_token)?;
        }
        // Capture only the explicitly chosen workflow. This reads the runtime;
        // archived records never enter its queue or restore execution authority.
        let snapshot = runtime(&app.state::<Collaboration>())?.snapshot()?;
        let history = History::capture(&snapshot, &[workflow_id]).map_err(|e| e.to_string())?;
        let mut session = state
            .0
            .lock()
            .map_err(|_| "文档会话无法访问，请重新启动应用")?;
        check_project(&session, &project_id, &document_token)?;
        let snapshot = session.attach_history(history)?;
        Ok(snapshot)
    })
    .await
    .map_err(|error| format!("协作历史未保存到项目：{error}"))?
}

#[tauri::command]
pub async fn project_archived_history(
    app: tauri::AppHandle,
    project_id: String,
    document_token: String,
) -> Result<History, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::AppState>();
        let session = state
            .0
            .lock()
            .map_err(|_| "文档会话无法访问，请重新启动应用")?;
        check_project(&session, &project_id, &document_token)?;
        Ok(session.archived_history().clone())
    })
    .await
    .map_err(|error| format!("协作历史读取失败：{error}"))?
}

fn check_project(
    session: &session::Session,
    project_id: &str,
    document_token: &str,
) -> Result<(), String> {
    if session.project_id() != project_id || session.document_token() != document_token {
        return Err("当前项目已切换，请在目标项目中重新操作".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn ai_command(app: tauri::AppHandle, command: Command) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        runtime(&app.state::<Collaboration>())?.command(command)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ai_scan(project: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || ai::scan_project(Path::new(&project)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ai_export(
    app: tauri::AppHandle,
    workflow_id: String,
    run_id: String,
    destination: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        runtime(&app.state::<Collaboration>())?.export(
            &workflow_id,
            &run_id,
            Path::new(&destination),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn ai_has_active(state: tauri::State<'_, Collaboration>) -> bool {
    runtime(&state).and_then(|r| r.snapshot()).is_ok_and(|s| {
        s.workflows.iter().any(|w| {
            w.runs.iter().any(|r| {
                matches!(
                    r.status,
                    Status::Queued | Status::Running | Status::Stopping | Status::StorageBlocked
                )
            })
        })
    })
}

#[tauri::command]
pub async fn ai_shutdown(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(runtime) = runtime(&app.state::<Collaboration>()) {
            runtime.shutdown()?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_requests_require_the_same_project_after_switching_documents() {
        let mut session = session::Session::new().unwrap();
        let original_id = session.project_id().to_owned();
        let original_token = session.document_token().to_owned();
        assert!(check_project(&session, &original_id, &original_token).is_ok());
        session
            .execute(session::Command::New { discard: false })
            .unwrap();
        assert!(check_project(&session, &original_id, &original_token).is_err());
        assert!(check_project(&session, session.project_id(), session.document_token()).is_ok());
    }

    #[test]
    fn opening_a_copy_with_the_same_project_id_rejects_the_previous_document_token() {
        let mut session = session::Session::new().unwrap();
        let original_id = session.project_id().to_owned();
        let original_token = session.document_token().to_owned();
        let directory = std::env::temp_dir().join(format!("ravomi-archive-{original_token}"));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("original.mio");
        let copy = directory.join("copy.mio");
        session
            .execute(session::Command::Save {
                path: path.to_string_lossy().into_owned(),
            })
            .unwrap();
        std::fs::copy(&path, &copy).unwrap();
        session
            .execute(session::Command::Open {
                path: copy.to_string_lossy().into_owned(),
                discard: false,
            })
            .unwrap();
        assert_eq!(session.project_id(), original_id);
        assert_ne!(session.document_token(), original_token);
        assert!(check_project(&session, &original_id, &original_token).is_err());
        assert!(check_project(&session, &original_id, session.document_token()).is_ok());
        std::fs::remove_dir_all(directory).unwrap();
    }
}

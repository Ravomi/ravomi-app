use project_graph_bevy_canvas::collaboration::{self as ai, Command, Runtime, Snapshot, Status};
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
    let runtime = Runtime::open(app.path().app_data_dir()?.join("code-collaboration"));
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

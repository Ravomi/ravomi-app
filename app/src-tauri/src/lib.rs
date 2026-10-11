use project_graph_bevy_canvas::session::{Command, Session, Snapshot};
use std::sync::Mutex;
use tauri::Manager;
mod collaboration;

struct AppState(Mutex<Session>);

#[tauri::command]
fn session_snapshot(state: tauri::State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(state
        .0
        .lock()
        .map_err(|_| "文档会话无法访问，请重新启动应用")?
        .snapshot())
}

#[tauri::command]
async fn session_execute(app: tauri::AppHandle, command: Command) -> Result<Snapshot, String> {
    // PRG IO and parsing must not block the webview event loop.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state
            .0
            .lock()
            .map_err(|_| "文档会话无法访问，请重新启动应用")?
            .execute(command)
    })
    .await
    .map_err(|error| format!("文档操作未完成：{error}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            collaboration::install(app)?;
            app.manage(AppState(Mutex::new(
                Session::new().map_err(std::io::Error::other)?,
            )));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            collaboration::ai_snapshot,
            collaboration::archive_project_workflow,
            collaboration::project_archived_history,
            collaboration::ai_configure,
            collaboration::ai_command,
            collaboration::ai_scan,
            collaboration::ai_export,
            collaboration::ai_has_active,
            collaboration::ai_shutdown,
            session_snapshot,
            session_execute
        ])
        .run(tauri::generate_context!())
}

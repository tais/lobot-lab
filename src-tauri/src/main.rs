mod animation;
mod filter;
mod loader;
mod model;
mod render;
mod sti;
mod vfs;
mod xml;

use std::sync::Mutex;

use loader::Workspace;
use model::{
    AttachmentOptionDto, AuditDto, PreviewContextDto, PreviewDto, PreviewRequest,
    WorkspaceSummaryDto,
};

#[derive(Default)]
struct AppState {
    workspace: Mutex<Option<Workspace>>,
}

#[tauri::command]
fn discover_data_roots(install_path: String) -> Result<Vec<String>, String> {
    vfs::discover_data_roots(&install_path)
}

#[tauri::command]
fn load_workspace(
    state: tauri::State<AppState>,
    roots: Vec<String>,
) -> Result<WorkspaceSummaryDto, String> {
    let workspace = Workspace::load(roots)?;
    let summary = workspace.summary();
    *state
        .workspace
        .lock()
        .map_err(|_| "Workspace lock was poisoned")? = Some(workspace);
    Ok(summary)
}

#[tauri::command]
fn preview_context(
    state: tauri::State<AppState>,
    request: PreviewRequest,
) -> Result<PreviewContextDto, String> {
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace lock was poisoned")?;
    guard
        .as_ref()
        .ok_or_else(|| "Load a data workspace first".to_string())?
        .preview_context(&request)
}

#[tauri::command]
fn render_preview(
    state: tauri::State<AppState>,
    request: PreviewRequest,
) -> Result<PreviewDto, String> {
    let mut guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace lock was poisoned")?;
    guard
        .as_mut()
        .ok_or_else(|| "Load a data workspace first".to_string())?
        .render_preview(&request)
}

#[tauri::command]
fn attachment_options(
    state: tauri::State<AppState>,
    host_id: u16,
) -> Result<Vec<AttachmentOptionDto>, String> {
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace lock was poisoned")?;
    Ok(guard
        .as_ref()
        .ok_or_else(|| "Load a data workspace first".to_string())?
        .attachment_options(host_id))
}

#[tauri::command]
fn audit_workspace(
    state: tauri::State<AppState>,
    request: PreviewRequest,
) -> Result<AuditDto, String> {
    let mut guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace lock was poisoned")?;
    guard
        .as_mut()
        .ok_or_else(|| "Load a data workspace first".to_string())?
        .audit_workspace(&request)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            discover_data_roots,
            load_workspace,
            preview_context,
            render_preview,
            attachment_options,
            audit_workspace
        ])
        .run(tauri::generate_context!())
        .expect("error while running LOBOT Lab");
}

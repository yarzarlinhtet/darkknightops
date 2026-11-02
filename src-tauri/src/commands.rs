use crate::{
    application::ServerService,
    domain::{AppError, ErrorCode, Server, ServerInput},
};
use std::sync::Arc;

pub struct AppState(pub Result<Arc<ServerService>, AppError>);
async fn run<T, F>(state: tauri::State<'_, AppState>, action: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce(&ServerService) -> Result<T, AppError> + Send + 'static,
{
    let service = state.0.clone()?;
    tauri::async_runtime::spawn_blocking(move || action(&service))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Local operation could not complete."))?
}
#[tauri::command]
pub async fn list_servers(state: tauri::State<'_, AppState>) -> Result<Vec<Server>, AppError> {
    run(state, |service| service.list()).await
}
#[tauri::command]
pub async fn get_server(state: tauri::State<'_, AppState>, id: String) -> Result<Server, AppError> {
    run(state, move |service| service.get(&id)).await
}
#[tauri::command]
pub async fn create_server(
    state: tauri::State<'_, AppState>,
    input: ServerInput,
) -> Result<Server, AppError> {
    run(state, move |service| service.create(&input)).await
}
#[tauri::command]
pub async fn update_server(
    state: tauri::State<'_, AppState>,
    id: String,
    input: ServerInput,
) -> Result<Server, AppError> {
    run(state, move |service| service.update(&id, &input)).await
}
#[tauri::command]
pub async fn delete_server(state: tauri::State<'_, AppState>, id: String) -> Result<(), AppError> {
    run(state, move |service| service.delete(&id)).await
}

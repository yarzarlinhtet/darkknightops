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
    key_id: Option<String>,
) -> Result<Server, AppError> {
    run(state, move |service| match key_id {
        Some(key) => service.save_with_key(None, &input, &key),
        None => service.create(&input),
    })
    .await
}
#[tauri::command]
pub async fn update_server(
    state: tauri::State<'_, AppState>,
    id: String,
    input: ServerInput,
    key_id: Option<String>,
) -> Result<Server, AppError> {
    run(state, move |service| match key_id {
        Some(key) => service.save_with_key(Some(&id), &input, &key),
        None => service.update(&id, &input),
    })
    .await
}
#[tauri::command]
pub async fn delete_server(state: tauri::State<'_, AppState>, id: String) -> Result<(), AppError> {
    run(state, move |service| service.delete(&id)).await
}

use crate::domain::connection::*;
fn connections(
    service: &ServerService,
) -> Result<&crate::application::connection::ConnectionService, AppError> {
    service
        .connections
        .as_deref()
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "SSH services unavailable."))
}
#[tauri::command]
pub async fn get_ssh_metadata(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<SshMetadata, AppError> {
    run(state, move |s| connections(s)?.metadata(&id)).await
}
#[tauri::command]
pub async fn select_ssh_key(
    state: tauri::State<'_, AppState>,
    id: Option<String>,
) -> Result<Option<String>, AppError> {
    run(state, move |s| {
        if let Some(id) = &id {
            s.get(id)?;
        }
        connections(s)?;
        rfd::FileDialog::new()
            .set_title("Select SSH private key")
            .pick_file()
            .map(|p| {
                crate::infrastructure::credentials::validate_key_path(&p.to_string_lossy())
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .transpose()
    })
    .await
}
#[tauri::command]
pub async fn configure_ssh_credential(
    state: tauri::State<'_, AppState>,
    id: String,
    key_path: Option<String>,
    passphrase: Option<String>,
    save_passphrase: bool,
) -> Result<(), AppError> {
    let passphrase = passphrase.map(zeroize::Zeroizing::new);
    run(state, move |s| {
        connections(s)?.configure(&id, key_path, passphrase, save_passphrase)
    })
    .await
}
#[tauri::command]
pub async fn unlock_ssh_key(
    state: tauri::State<'_, AppState>,
    id: String,
    lease: String,
    passphrase: String,
) -> Result<(), AppError> {
    let passphrase = zeroize::Zeroizing::new(passphrase);
    run(state, move |s| {
        connections(s)?.unlock(&id, &lease, passphrase)
    })
    .await
}
#[tauri::command]
pub async fn open_server_session(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<String, AppError> {
    run(state, move |s| connections(s)?.open(&id)).await
}
#[tauri::command]
pub async fn close_server_session(
    state: tauri::State<'_, AppState>,
    id: String,
    lease: String,
) -> Result<(), AppError> {
    run(state, move |s| connections(s)?.close(&id, &lease)).await
}
#[tauri::command]
pub async fn cancel_server_operation(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    run(state, move |s| connections(s)?.cancel(&id)).await
}
#[tauri::command]
pub async fn test_server_connection(
    state: tauri::State<'_, AppState>,
    id: String,
    lease: Option<String>,
) -> Result<ConnectionTestResult, AppError> {
    run(state, move |s| connections(s)?.test(&id, lease.as_deref())).await
}
#[tauri::command]
pub async fn trust_server_host_key(
    state: tauri::State<'_, AppState>,
    id: String,
    observation_id: String,
    replace: bool,
) -> Result<(), AppError> {
    run(state, move |s| {
        connections(s)?.trust(&id, &observation_id, replace)
    })
    .await
}
#[tauri::command]
pub async fn get_server_info(
    state: tauri::State<'_, AppState>,
    id: String,
    lease: String,
) -> Result<ServerInfo, AppError> {
    run(state, move |s| connections(s)?.info(&id, &lease)).await
}
#[tauri::command]
pub async fn get_docker_capabilities(
    state: tauri::State<'_, AppState>,
    id: String,
    lease: String,
) -> Result<DockerCapabilities, AppError> {
    run(state, move |s| connections(s)?.docker(&id, &lease)).await
}
#[tauri::command]
pub async fn get_server_metrics(
    state: tauri::State<'_, AppState>,
    id: String,
    lease: String,
) -> Result<ServerMetrics, AppError> {
    run(state, move |s| connections(s)?.metrics(&id, &lease)).await
}

fn keys(service: &ServerService) -> Result<&crate::application::keys::KeyService, AppError> {
    service
        .keys
        .as_deref()
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "SSH key service unavailable."))
}
#[tauri::command]
pub async fn list_ssh_keys(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::application::keys::SshKey>, AppError> {
    run(state, |s| keys(s)?.list()).await
}
#[tauri::command]
pub async fn get_ssh_key(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<crate::application::keys::SshKey, AppError> {
    run(state, move |s| keys(s)?.get(&id)).await
}
#[tauri::command]
pub async fn generate_ssh_key(
    state: tauri::State<'_, AppState>,
    name: String,
    rsa: bool,
    passphrase: Option<String>,
    save_passphrase: bool,
) -> Result<crate::application::keys::SshKey, AppError> {
    let secret = passphrase.map(zeroize::Zeroizing::new);
    run(state, move |s| {
        keys(s)?.create(
            &name,
            rsa,
            None,
            true,
            secret.as_deref().map(|s| s.as_str()),
            save_passphrase,
        )
    })
    .await
}
#[tauri::command]
pub async fn import_ssh_key(
    state: tauri::State<'_, AppState>,
    name: String,
    path: String,
    managed: bool,
    passphrase: Option<String>,
    save_passphrase: bool,
) -> Result<crate::application::keys::SshKey, AppError> {
    let secret = passphrase.map(zeroize::Zeroizing::new);
    run(state, move |s| {
        keys(s)?.create(
            &name,
            false,
            Some(&path),
            managed,
            secret.as_deref().map(|s| s.as_str()),
            save_passphrase,
        )
    })
    .await
}
#[tauri::command]
pub async fn rename_ssh_key(
    state: tauri::State<'_, AppState>,
    id: String,
    name: String,
) -> Result<crate::application::keys::SshKey, AppError> {
    run(state, move |s| keys(s)?.rename(&id, &name)).await
}
#[tauri::command]
pub async fn delete_ssh_key(
    state: tauri::State<'_, AppState>,
    id: String,
    confirmed: bool,
    delete_file: bool,
) -> Result<(), AppError> {
    run(state, move |s| keys(s)?.delete(&id, confirmed, delete_file)).await
}
#[tauri::command]
pub async fn get_ssh_public_key(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<String, AppError> {
    run(state, move |s| Ok(keys(s)?.get(&id)?.public_key)).await
}
#[tauri::command]
pub async fn export_ssh_public_key(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<bool, AppError> {
    run(state, move |s| {
        let key = keys(s)?.get(&id)?;
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("ssh-key.pub")
            .add_filter("Public key", &["pub"])
            .save_file()
        else {
            return Ok(false);
        };
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| {
                AppError::validation(
                    "file",
                    "Choose a new public-key filename; existing files are not overwritten.",
                )
            })?;
        writeln!(file, "{}", key.public_key).map_err(|_| {
            AppError::new(
                ErrorCode::KeyFileUnavailable,
                "Could not export public key.",
            )
        })?;
        Ok(true)
    })
    .await
}
#[tauri::command]
pub async fn assign_ssh_key_to_server(
    state: tauri::State<'_, AppState>,
    id: String,
    key_id: Option<String>,
) -> Result<(), AppError> {
    run(state, move |s| {
        connections(s)?.assign_key(&id, key_id.as_deref())
    })
    .await
}
#[tauri::command]
pub async fn list_servers_using_ssh_key(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Vec<Server>, AppError> {
    run(state, move |s| {
        keys(s)?.servers(&id)?.iter().map(|id| s.get(id)).collect()
    })
    .await
}
#[tauri::command]
pub async fn get_server_ssh_key_id(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Option<String>, AppError> {
    run(state, move |s| keys(s)?.assigned(&id)).await
}

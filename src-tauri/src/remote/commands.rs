//! Tauri command adapters for remote control. Thin: every rule lives in the service, the store and
//! the auth client. All async, so nothing here runs on Tauri's main thread.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::auth::Account;
use super::pairing::PendingDevice;
use super::service::{RemoteService, RemoteStatus};
use super::store::{self, PairedDevice, RemoteRoot};

async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

// Re-sync the service with the saved settings (remoteEnabled). Called by set_settings after every
// save; spawned so the settings write never waits on the network.
pub fn apply_settings(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(service) = app.try_state::<RemoteService>() {
            service.apply().await;
        }
    });
}

#[tauri::command]
pub fn remote_status(service: State<'_, RemoteService>) -> RemoteStatus {
    service.status()
}

// The signed-in sito-auth account, or null when signed out.
#[tauri::command]
pub async fn remote_account(service: State<'_, RemoteService>) -> Result<Option<Account>, String> {
    service.auth.account().await
}

#[tauri::command]
pub async fn remote_sign_in(
    service: State<'_, RemoteService>,
    email: String,
    password: String,
) -> Result<Account, String> {
    let account = service.auth.sign_in(email.trim(), &password).await?;
    service.apply().await;
    Ok(account)
}

// Stop serving first, then revoke the session on sito-auth and forget it locally.
#[tauri::command]
pub async fn remote_sign_out(service: State<'_, RemoteService>) -> Result<(), String> {
    service.stop().await;
    let result = service.auth.sign_out().await;
    service.apply().await;
    result
}

// Authorized roots, with their absolute paths (desktop UI only; the phone never sees `path`).
#[tauri::command]
pub async fn remote_get_roots(
    service: State<'_, RemoteService>,
) -> Result<Vec<RemoteRoot>, String> {
    let config_dir = service.config_dir()?;
    blocking(move || store::read_config_from(&config_dir).map(|config| config.roots)).await
}

#[tauri::command]
pub async fn remote_add_root(
    app: AppHandle,
    service: State<'_, RemoteService>,
    path: String,
) -> Result<RemoteRoot, String> {
    let config_dir = service.config_dir()?;
    let home = app.path().home_dir().ok();
    let root = blocking(move || {
        store::add_root(&config_dir, &PathBuf::from(path.trim()), home.as_deref())
    })
    .await?;
    service.reload_config().await;
    Ok(root)
}

#[tauri::command]
pub async fn remote_remove_root(
    service: State<'_, RemoteService>,
    id: String,
) -> Result<(), String> {
    let config_dir = service.config_dir()?;
    blocking(move || store::remove_root(&config_dir, &id)).await?;
    service.reload_config().await;
    Ok(())
}

#[tauri::command]
pub async fn remote_get_devices(
    service: State<'_, RemoteService>,
) -> Result<Vec<PairedDevice>, String> {
    let config_dir = service.config_dir()?;
    blocking(move || store::read_config_from(&config_dir).map(|config| config.devices)).await
}

#[tauri::command]
pub async fn remote_revoke_device(
    service: State<'_, RemoteService>,
    id: String,
) -> Result<(), String> {
    service.revoke_device(&id).await
}

// Pixel edge of the rendered QR; the dialog scales the SVG to fit.
const QR_MIN_SIZE: u32 = 240;
const ERROR_QR: &str = "REMOTE_PAIRING_QR_FAILED";

// What the pairing dialog shows: the QR as SVG markup plus the raw payload (for debugging/tests).
#[derive(Serialize)]
pub struct PairingCode {
    payload: String,
    svg: String,
}

// Open a 2-minute, one-time pairing window and render its QR.
#[tauri::command]
pub async fn remote_start_pairing(
    service: State<'_, RemoteService>,
) -> Result<PairingCode, String> {
    let payload = service.start_pairing().await?.to_json();
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|_| ERROR_QR.to_string())?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(QR_MIN_SIZE, QR_MIN_SIZE)
        .quiet_zone(true)
        .build();
    Ok(PairingCode { payload, svg })
}

// The phone waiting for the user to type its code, if any (also pushed as the remote://pairing
// event). The SAS itself never reaches the UI.
#[tauri::command]
pub fn remote_pending_pairing(service: State<'_, RemoteService>) -> Option<PendingDevice> {
    service.pending_pairing()
}

#[tauri::command]
pub async fn remote_confirm_pairing(
    service: State<'_, RemoteService>,
    device_id: String,
    code: String,
) -> Result<PairedDevice, String> {
    service.confirm_pairing(&device_id, &code).await
}

#[tauri::command]
pub async fn remote_cancel_pairing(service: State<'_, RemoteService>) -> Result<(), String> {
    service.cancel_pairing().await;
    Ok(())
}

//! Remote service lifecycle. Owns the sito-auth client, the desktop identity, the relay client and
//! the session loop. Managed as Tauri state; every piece of work runs on the async runtime or the
//! blocking pool, never on Tauri's main thread.
//!
//! Security rule 3: nothing connects to the relay unless `remoteEnabled` is on and the user is
//! signed in. Turning the setting off tears everything down.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use super::auth::{AuthClient, ERROR_DEVICE_CONFLICT};
use super::crypto::KeyPair;
use super::error::RemoteError;
use super::handlers::{self, Request, Response, ServeOptions};
use super::pairing::{Pairing, PairingQr, PendingDevice, CODE_LEN};
use super::relay_client::{self, RelayEvent, RelayHandle, StopReason};
use super::secrets;
use super::session::{reject, Action, SessionEngine};
use super::store::{self, PairedDevice, RemoteConfig};
use super::wire::RejectCode;
use crate::functions::settings::get_settings;

// Events for the frontend (RemoteProvider).
pub const STATUS_EVENT: &str = "remote://status";
pub const PAIRING_EVENT: &str = "remote://pairing";
pub const SESSION_OPENED_EVENT: &str = "remote://session-opened";

pub const ERROR_NOT_RUNNING: &str = "REMOTE_NOT_RUNNING";
const ERROR_IDENTITY: &str = "REMOTE_IDENTITY_FAILED";
const ERROR_NO_PENDING: &str = "REMOTE_NO_PENDING_PAIRING";
const ERROR_CODE_MISMATCH: &str = "REMOTE_PAIRING_CODE_MISMATCH";

const DESKTOP_KIND: &str = "DESKTOP";
const MOBILE_KIND: &str = "MOBILE";
const ANDROID_PLATFORM: &str = "ANDROID";
const FALLBACK_DEVICE_NAME: &str = "Sito File Browser";
const LOOP_QUEUE: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteState {
    Off,
    Starting,
    Connecting,
    Online,
    // Stopped by itself; `reason` says why (signed out, replaced, rejected, setup error).
    Stopped,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OnlinePhone {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub state: RemoteState,
    pub reason: Option<String>,
    // Paired phones currently online on the relay.
    pub online: Vec<OnlinePhone>,
    // Phones with an open, authenticated session right now (the visible indicator).
    pub sessions: Vec<String>,
}

impl Default for RemoteStatus {
    fn default() -> Self {
        Self {
            state: RemoteState::Off,
            reason: None,
            online: Vec::new(),
            sessions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionOpened {
    device_id: String,
    name: String,
}

// Commands from the Tauri adapters and from spawned tasks into the session loop.
enum LoopCommand {
    Stop,
    ReloadConfig(Arc<RemoteConfig>),
    FinishPairing {
        device_id: u64,
        accepted: bool,
    },
    DropDevice(u64),
    PairingChecked {
        from: u64,
        sid: String,
        ephemeral: [u8; super::crypto::POINT_LEN],
        static_spki: Vec<u8>,
        code: [u8; CODE_LEN],
        name: Option<String>,
    },
    Served {
        from: u64,
        sid: String,
        id: u64,
        result: Result<Response, RemoteError>,
    },
}

struct Running {
    commands: mpsc::Sender<LoopCommand>,
    identity: Arc<KeyPair>,
    desktop_device_id: String,
}

// Shared between the loop and the command adapters. Locked briefly, never across an await.
#[derive(Default)]
struct Shared {
    status: RemoteStatus,
    pairing: Pairing,
}

pub struct RemoteService {
    app: AppHandle,
    pub auth: Arc<AuthClient>,
    // Serializes start/stop so two quick toggles cannot start two relay clients.
    running: tokio::sync::Mutex<Option<Running>>,
    shared: Arc<Mutex<Shared>>,
}

impl RemoteService {
    pub fn new(app: AppHandle) -> Result<Self, String> {
        Ok(Self {
            app,
            auth: Arc::new(AuthClient::new()?),
            running: tokio::sync::Mutex::new(None),
            shared: Arc::new(Mutex::new(Shared::default())),
        })
    }

    pub fn status(&self) -> RemoteStatus {
        self.shared
            .lock()
            .map(|shared| shared.status.clone())
            .unwrap_or_default()
    }

    pub fn pending_pairing(&self) -> Option<PendingDevice> {
        self.shared.lock().ok()?.pairing.pending().cloned()
    }

    // Bring the service in line with the settings: start when enabled (and signed in), stop when
    // disabled. Idempotent; called at launch, after a settings save and after sign-in.
    pub async fn apply(&self) {
        let app = self.app.clone();
        let enabled = tauri::async_runtime::spawn_blocking(move || {
            get_settings(app).remote_enabled
        })
        .await
        .unwrap_or(false);

        let mut running = self.running.lock().await;
        let stopped = self.status().state == RemoteState::Stopped;
        if !enabled || stopped {
            if let Some(previous) = running.take() {
                let _ = previous.commands.send(LoopCommand::Stop).await;
            }
            if let Ok(mut shared) = self.shared.lock() {
                shared.pairing.cancel();
            }
        }
        if !enabled {
            self.set_status(RemoteState::Off, None);
            return;
        }
        if running.is_some() {
            return;
        }
        self.set_status(RemoteState::Starting, None);
        match self.start().await {
            Ok(started) => *running = Some(started),
            Err(reason) => self.set_status(RemoteState::Stopped, Some(reason)),
        }
    }

    pub async fn stop(&self) {
        if let Some(previous) = self.running.lock().await.take() {
            let _ = previous.commands.send(LoopCommand::Stop).await;
        }
        if let Ok(mut shared) = self.shared.lock() {
            shared.pairing.cancel();
        }
        self.set_status(RemoteState::Off, None);
    }

    pub async fn start_pairing(&self) -> Result<PairingQr, String> {
        let running = self.running.lock().await;
        let running = running.as_ref().ok_or_else(|| ERROR_NOT_RUNNING.to_string())?;
        let mut shared = self.shared.lock().map_err(|e| e.to_string())?;
        shared.pairing.start(
            &running.desktop_device_id,
            &running.identity.spki(),
            Instant::now(),
        )
    }

    // The user typed the SAS the phone shows. A match trusts the phone and tells it (pair.ok); a
    // mismatch rejects it (pair.rejected). One attempt either way: the pending device is consumed.
    pub async fn confirm_pairing(
        &self,
        device_id: &str,
        code: &str,
    ) -> Result<PairedDevice, String> {
        let pending = self
            .shared
            .lock()
            .map_err(|e| e.to_string())?
            .pairing
            .take_pending(device_id)
            .ok_or_else(|| ERROR_NO_PENDING.to_string())?;
        if !pending.sas_matches(code) {
            if let Ok(id) = device_id.parse() {
                self.send(LoopCommand::FinishPairing {
                    device_id: id,
                    accepted: false,
                })
                .await;
            }
            return Err(ERROR_CODE_MISMATCH.to_string());
        }
        let device = PairedDevice {
            id: pending.device_id.clone(),
            name: pending.name,
            platform: ANDROID_PLATFORM.to_string(),
            public_key: pending.public_key,
            paired_at: unix_seconds(),
        };
        let config_dir = self.config_dir()?;
        let stored = device.clone();
        let config = blocking(move || {
            store::add_device(&config_dir, stored)?;
            store::read_config_from(&config_dir)
        })
        .await?;
        self.send(LoopCommand::ReloadConfig(Arc::new(config))).await;
        if let Ok(id) = device.id.parse() {
            self.send(LoopCommand::FinishPairing {
                device_id: id,
                accepted: true,
            })
            .await;
        }
        Ok(device)
    }

    pub async fn cancel_pairing(&self) {
        let pending = self.shared.lock().ok().and_then(|mut shared| {
            let pending = shared.pairing.pending().cloned();
            shared.pairing.cancel();
            pending
        });
        if let Some(id) = pending.and_then(|device| device.device_id.parse().ok()) {
            self.send(LoopCommand::FinishPairing {
                device_id: id,
                accepted: false,
            })
            .await;
        }
    }

    // Revoke a paired phone: local trust first (immediate), then its live session, then sito-auth
    // (best effort; the local removal alone already blocks it).
    pub async fn revoke_device(&self, device_id: &str) -> Result<(), String> {
        let config_dir = self.config_dir()?;
        let id = device_id.to_string();
        let config = blocking(move || {
            store::remove_device(&config_dir, &id)?;
            store::read_config_from(&config_dir)
        })
        .await?;
        self.send(LoopCommand::ReloadConfig(Arc::new(config))).await;
        if let Ok(id) = device_id.parse() {
            self.send(LoopCommand::DropDevice(id)).await;
        }
        let _ = self.auth.revoke_device(device_id).await;
        Ok(())
    }

    // remote.toml changed (roots added/removed): refresh the loop's copy.
    pub async fn reload_config(&self) {
        let Ok(config_dir) = self.config_dir() else {
            return;
        };
        if let Ok(config) = blocking(move || store::read_config_from(&config_dir)).await {
            self.send(LoopCommand::ReloadConfig(Arc::new(config))).await;
        }
    }

    pub fn config_dir(&self) -> Result<PathBuf, String> {
        self.app.path().app_config_dir().map_err(|e| e.to_string())
    }

    async fn send(&self, command: LoopCommand) {
        if let Some(running) = self.running.lock().await.as_ref() {
            let _ = running.commands.send(command).await;
        }
    }

    async fn start(&self) -> Result<Running, String> {
        // Signed out → Stopped with the auth code; the UI shows the sign-in form.
        self.auth.account().await?.ok_or_else(|| {
            super::auth::ERROR_SIGNED_OUT.to_string()
        })?;
        let identity = Arc::new(load_identity().await?);
        let config_dir = self.config_dir()?;
        let desktop_device_id = self.ensure_registered(&identity, config_dir.clone()).await?;
        let device_id: u64 = desktop_device_id
            .parse()
            .map_err(|_| ERROR_IDENTITY.to_string())?;
        let config = blocking(move || store::read_config_from(&config_dir)).await?;

        let (events_tx, events_rx) = mpsc::channel(LOOP_QUEUE);
        let (commands_tx, commands_rx) = mpsc::channel(LOOP_QUEUE);
        let relay = relay_client::spawn(self.auth.clone(), device_id, events_tx);
        self.set_status(RemoteState::Connecting, None);

        let session_loop = SessionLoop {
            app: self.app.clone(),
            auth: self.auth.clone(),
            shared: self.shared.clone(),
            engine: SessionEngine::new(identity.clone()),
            config: Arc::new(config),
            relay,
            commands: commands_tx.clone(),
        };
        tauri::async_runtime::spawn(session_loop.run(events_rx, commands_rx));
        Ok(Running {
            commands: commands_tx,
            identity,
            desktop_device_id,
        })
    }

    // This desktop's sito-auth device id: reuse the stored one while the registry still has it
    // with our key, otherwise register (or recover it after a 409).
    async fn ensure_registered(
        &self,
        identity: &KeyPair,
        config_dir: PathBuf,
    ) -> Result<String, String> {
        let spki = BASE64.encode(identity.spki());
        let dir = config_dir.clone();
        let config = blocking(move || store::read_config_from(&dir)).await?;
        if let Some(id) = config.desktop_device_id {
            if let Some(device) = self.auth.device(&id).await? {
                if device.public_key == spki {
                    return Ok(id);
                }
            }
        }
        let name = crate::filesystem::volumes::get_host_name()
            .unwrap_or_else(|| FALLBACK_DEVICE_NAME.to_string());
        let id = match self.auth.register_device(&name, &spki).await {
            Ok(id) => id,
            Err(code) if code == ERROR_DEVICE_CONFLICT => self
                .auth
                .devices()
                .await?
                .into_iter()
                .find(|device| device.kind == DESKTOP_KIND && device.public_key == spki)
                .map(|device| device.id.to_string())
                .ok_or(code)?,
            Err(code) => return Err(code),
        };
        let stored = id.clone();
        blocking(move || store::set_desktop_device_id(&config_dir, Some(stored))).await?;
        Ok(id)
    }

    fn set_status(&self, state: RemoteState, reason: Option<String>) {
        set_status(&self.app, &self.shared, |status| {
            status.state = state;
            status.reason = reason;
            if state != RemoteState::Online && state != RemoteState::Connecting {
                status.online.clear();
                status.sessions.clear();
            }
        });
    }
}

// The session loop: owns the engine and the relay handle (dropping it stops the relay client).
struct SessionLoop {
    app: AppHandle,
    auth: Arc<AuthClient>,
    shared: Arc<Mutex<Shared>>,
    engine: SessionEngine,
    config: Arc<RemoteConfig>,
    relay: RelayHandle,
    commands: mpsc::Sender<LoopCommand>,
}

impl SessionLoop {
    async fn run(
        mut self,
        mut events: mpsc::Receiver<RelayEvent>,
        mut commands: mpsc::Receiver<LoopCommand>,
    ) {
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => {
                        if !self.on_event(event) {
                            return;
                        }
                    }
                    None => return,
                },
                command = commands.recv() => match command {
                    Some(LoopCommand::Stop) | None => return,
                    Some(command) => self.on_command(command),
                },
            }
        }
    }

    // Returns false when the relay client stopped for good.
    fn on_event(&mut self, event: RelayEvent) -> bool {
        match event {
            RelayEvent::Connected => self.update(|status| {
                status.state = RemoteState::Online;
                status.reason = None;
            }),
            RelayEvent::Disconnected => {
                // Sessions cannot survive the socket: the phones re-hello after reconnecting.
                for id in self.engine.active_devices() {
                    let actions = self.engine.close(id);
                    self.perform(actions);
                }
                self.update(|status| {
                    status.state = RemoteState::Connecting;
                    status.online.clear();
                    status.sessions.clear();
                });
            }
            RelayEvent::Presence(devices) => {
                let config = self.config.clone();
                self.update(|status| {
                    status.online = devices
                        .iter()
                        .filter(|device| device.kind == MOBILE_KIND)
                        .map(|device| OnlinePhone {
                            id: device.id.to_string(),
                            name: phone_name(&config, device.id, &device.name),
                        })
                        .collect();
                });
            }
            RelayEvent::PresenceUpdate { device, online } => {
                if device.kind != MOBILE_KIND {
                    return true;
                }
                let id = device.id.to_string();
                let name = phone_name(&self.config, device.id, &device.name);
                if !online {
                    let actions = self.engine.close(device.id);
                    self.perform(actions);
                }
                self.update(|status| {
                    status.online.retain(|phone| phone.id != id);
                    if online {
                        status.online.push(OnlinePhone { id, name });
                    }
                });
            }
            RelayEvent::Route(route) => {
                let actions = match self.shared.lock() {
                    Ok(mut shared) => {
                        self.engine
                            .on_route(route, &self.config, &mut shared.pairing, Instant::now())
                    }
                    Err(_) => Vec::new(),
                };
                self.perform(actions);
            }
            // Relay-level errors (NOT_ONLINE, RATE_LIMITED...) concern single frames; the phone
            // retries on its side.
            RelayEvent::Error { .. } => {}
            RelayEvent::Stopped(reason) => {
                let reason = match reason {
                    StopReason::SignedOut => super::auth::ERROR_SIGNED_OUT,
                    StopReason::Replaced => "REMOTE_REPLACED",
                    StopReason::Rejected => "REMOTE_DEVICE_REJECTED",
                    StopReason::Requested => return false,
                };
                self.update(|status| {
                    status.state = RemoteState::Stopped;
                    status.reason = Some(reason.to_string());
                    status.online.clear();
                    status.sessions.clear();
                });
                return false;
            }
        }
        true
    }

    fn on_command(&mut self, command: LoopCommand) {
        let actions = match command {
            LoopCommand::Stop => Vec::new(),
            LoopCommand::ReloadConfig(config) => {
                // A root removal must not keep serving through sessions started before it: the
                // handlers read roots from this copy on every request.
                self.config = config;
                Vec::new()
            }
            LoopCommand::FinishPairing {
                device_id,
                accepted,
            } => self.engine.finish_pairing(device_id, accepted),
            LoopCommand::DropDevice(device_id) => self.engine.close(device_id),
            LoopCommand::PairingChecked {
                from,
                sid,
                ephemeral,
                static_spki,
                code,
                name,
            } => match name {
                Some(name) => {
                    self.engine
                        .accept_pairing(from, &sid, ephemeral, &static_spki, &code, name)
                }
                None => vec![reject(from, &sid, RejectCode::DeviceMismatch)],
            },
            LoopCommand::Served {
                from,
                sid,
                id,
                result,
            } => self.engine.respond(from, &sid, id, result),
        };
        self.perform(actions);
    }

    fn perform(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Send(outbound) => {
                    let _ = self.relay.send(outbound);
                }
                Action::VerifyPairing {
                    from,
                    sid,
                    ephemeral,
                    static_spki,
                    code,
                } => {
                    let auth = self.auth.clone();
                    let commands = self.commands.clone();
                    tauri::async_runtime::spawn(async move {
                        let name = verify_phone(&auth, from, &static_spki).await;
                        let _ = commands
                            .send(LoopCommand::PairingChecked {
                                from,
                                sid,
                                ephemeral,
                                static_spki,
                                code,
                                name,
                            })
                            .await;
                    });
                }
                Action::Serve {
                    from,
                    sid,
                    id,
                    request,
                } => self.serve(from, sid, id, request),
                Action::PairingProved {
                    device_id,
                    name,
                    public_key,
                    sas,
                } => {
                    let pending = PendingDevice {
                        device_id: device_id.to_string(),
                        name,
                        platform: ANDROID_PLATFORM.to_string(),
                        public_key,
                        sas,
                    };
                    if let Ok(mut shared) = self.shared.lock() {
                        shared.pairing.set_pending(pending.clone());
                    }
                    let _ = self.app.emit(PAIRING_EVENT, pending);
                }
                Action::SessionOpened { device_id } => {
                    let opened = SessionOpened {
                        device_id: device_id.to_string(),
                        name: phone_name(&self.config, device_id, ""),
                    };
                    let _ = self.app.emit(SESSION_OPENED_EVENT, opened);
                    self.sync_sessions();
                }
                Action::SessionClosed { .. } => self.sync_sessions(),
            }
        }
    }

    // Run one request on the blocking pool (inside handlers::handle) and feed the result back.
    fn serve(&self, from: u64, sid: String, id: u64, request: Request) {
        let app = self.app.clone();
        let config = self.config.clone();
        let commands = self.commands.clone();
        tauri::async_runtime::spawn(async move {
            let settings_app = app.clone();
            let show_hidden = tauri::async_runtime::spawn_blocking(move || {
                get_settings(settings_app).remote_show_hidden
            })
            .await
            .unwrap_or(false);
            let options = ServeOptions { show_hidden };
            let result = handlers::handle(&app, &config, options, request).await;
            let _ = commands
                .send(LoopCommand::Served {
                    from,
                    sid,
                    id,
                    result,
                })
                .await;
        });
    }

    fn sync_sessions(&self) {
        let sessions: Vec<String> = self
            .engine
            .active_devices()
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        self.update(|status| status.sessions = sessions);
    }

    fn update(&self, change: impl FnOnce(&mut RemoteStatus)) {
        set_status(&self.app, &self.shared, change);
    }
}

fn set_status(app: &AppHandle, shared: &Mutex<Shared>, change: impl FnOnce(&mut RemoteStatus)) {
    let snapshot = match shared.lock() {
        Ok(mut shared) => {
            change(&mut shared.status);
            shared.status.clone()
        }
        Err(_) => return,
    };
    let _ = app.emit(STATUS_EVENT, snapshot);
}

// Registry check for a pairing phone: it must exist for this user, be a phone, and hold exactly
// the static key it presented. Returns its registry name when it passes.
async fn verify_phone(auth: &AuthClient, from: u64, static_spki: &[u8]) -> Option<String> {
    let device = auth.device(&from.to_string()).await.ok()??;
    let matches = device.kind == MOBILE_KIND && device.public_key == BASE64.encode(static_spki);
    matches.then_some(device.name)
}

// Prefer the name stored at pairing time over what the relay reports.
fn phone_name(config: &RemoteConfig, id: u64, fallback: &str) -> String {
    config
        .device(&id.to_string())
        .map(|device| device.name.clone())
        .unwrap_or_else(|| fallback.to_string())
}

// The desktop's static key from the OS keyring, created and stored on first use.
async fn load_identity() -> Result<KeyPair, String> {
    blocking(|| {
        if let Some(stored) = secrets::get(secrets::DEVICE_PRIVATE_KEY)? {
            let der = BASE64
                .decode(stored)
                .map_err(|_| ERROR_IDENTITY.to_string())?;
            return KeyPair::from_der(&der).map_err(|_| ERROR_IDENTITY.to_string());
        }
        let key = KeyPair::generate().map_err(|_| ERROR_IDENTITY.to_string())?;
        let der = key.to_der().map_err(|_| ERROR_IDENTITY.to_string())?;
        secrets::set(secrets::DEVICE_PRIVATE_KEY, &BASE64.encode(der))?;
        Ok(key)
    })
    .await
}

async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

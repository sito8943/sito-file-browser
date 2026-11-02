use std::time::{Duration, Instant, SystemTime};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use super::secrets;

// sito-auth public base URL. Contract: servers/auth/readme.md (`/auth/*` and `/devices`).
pub const AUTH_BASE_URL: &str = "https://api.auth.sito8943.com";

// Bounded so a dead network never leaves the Remote settings spinner (or a relay reconnect)
// waiting forever.
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
// Refresh this long before the access token's `exp`, so a token never expires mid-request or
// mid-relay-handshake.
pub(crate) const ACCESS_REFRESH_MARGIN: Duration = Duration::from_secs(60);
// Used only when the token's `exp` cannot be read. Shorter than sito-auth's 15 minute lifetime.
const ACCESS_FALLBACK_LIFETIME: Duration = Duration::from_secs(10 * 60);

// Stable error codes the frontend maps to localized copy.
pub const ERROR_INVALID_CREDENTIALS: &str = "REMOTE_AUTH_INVALID_CREDENTIALS";
// No usable session: never signed in, signed out, or the refresh token was rejected/revoked.
pub const ERROR_SIGNED_OUT: &str = "REMOTE_AUTH_SIGNED_OUT";
pub const ERROR_NETWORK: &str = "REMOTE_AUTH_NETWORK";
pub const ERROR_RATE_LIMITED: &str = "REMOTE_AUTH_RATE_LIMITED";
pub const ERROR_SERVER: &str = "REMOTE_AUTH_SERVER";
// POST /devices 409: 20 active devices already, or this public key is already registered.
pub const ERROR_DEVICE_CONFLICT: &str = "REMOTE_DEVICE_CONFLICT";

// sito-auth AuthDto (sign-in / refresh / session). Only the fields remote control uses.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthDto {
    id: u64,
    #[serde(default)]
    username: Option<String>,
    email: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

// sito-auth DeviceDTO. `id` is numeric on the server; it travels as a string everywhere else
// (remote.toml, the relay's X-Device-Id header, the protocol).
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    pub id: u64,
    pub name: String,
    pub platform: String,
    pub kind: String,
    pub public_key: String,
}

// The signed-in account as the Remote settings section shows it.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub user_id: u64,
    pub email: String,
    pub username: Option<String>,
}

#[derive(Deserialize)]
struct JwtClaims {
    exp: u64,
}

struct CachedAccess {
    token: String,
    refresh_at: Instant,
}

#[derive(Default)]
struct TokenState {
    access: Option<CachedAccess>,
    account: Option<Account>,
}

enum Failure {
    Network,
    Status(StatusCode),
}

// sito-auth client. One instance lives in the remote service state. The access token stays in
// memory only; the refresh token lives in the OS keyring (secrets.rs).
//
// sito-auth rotates refresh tokens: every /auth/refresh revokes the one sent and returns a new
// one. Refreshes are therefore serialized behind `state` (two concurrent refreshes would revoke
// each other's token), and the new token is written to the keyring before the access token is
// used. If the response is lost after the server rotated, the stored token is already revoked and
// the next refresh answers SIGNED_OUT: the user signs in again, nothing is leaked.
pub struct AuthClient {
    http: reqwest::Client,
    base_url: String,
    state: Mutex<TokenState>,
}

impl AuthClient {
    pub fn new() -> Result<Self, String> {
        Self::with_base_url(AUTH_BASE_URL)
    }

    pub fn with_base_url(base_url: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent(concat!("sito-file-browser/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ERROR_NETWORK.to_string())?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            state: Mutex::new(TokenState::default()),
        })
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> Result<Account, String> {
        let mut state = self.state.lock().await;
        let body = json!({ "email": email, "password": password, "rememberMe": true });
        let dto: AuthDto = self
            .request(Method::POST, "/auth/sign-in", None, Some(&body))
            .await
            .map_err(|failure| match failure {
                Failure::Status(status) if is_rejection(status) => {
                    ERROR_INVALID_CREDENTIALS.to_string()
                }
                other => failure_code(other),
            })?;
        self.accept_tokens(&mut state, dto).await
    }

    // A valid access token, refreshing it first when it is missing or about to expire.
    pub async fn access_token(&self) -> Result<String, String> {
        let mut state = self.state.lock().await;
        if let Some(access) = &state.access {
            if Instant::now() < access.refresh_at {
                return Ok(access.token.clone());
            }
        }
        self.refresh(&mut state).await?;
        state
            .access
            .as_ref()
            .map(|access| access.token.clone())
            .ok_or_else(|| ERROR_SIGNED_OUT.to_string())
    }

    // The signed-in account, or None when signed out. Restores the session from the keyring on
    // first use after launch (one refresh).
    pub async fn account(&self) -> Result<Option<Account>, String> {
        if let Some(account) = self.state.lock().await.account.clone() {
            return Ok(Some(account));
        }
        match self.access_token().await {
            Ok(_) => Ok(self.state.lock().await.account.clone()),
            Err(code) if code == ERROR_SIGNED_OUT => Ok(None),
            Err(code) => Err(code),
        }
    }

    // Revoke the session on sito-auth (best effort: offline sign-out must still work locally),
    // then forget it locally.
    pub async fn sign_out(&self) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if let Some(refresh_token) = keyring_get(secrets::REFRESH_TOKEN).await? {
            let bearer = state.access.as_ref().map(|access| access.token.clone());
            let body = json!({ "refreshToken": refresh_token });
            let _ = self
                .send(Method::POST, "/auth/sign-out", bearer.as_deref(), Some(&body))
                .await;
        }
        *state = TokenState::default();
        keyring_delete(secrets::REFRESH_TOKEN).await
    }

    // Register this desktop (POST /devices). Returns the sito-auth device id as a string.
    pub async fn register_device(&self, name: &str, public_key: &str) -> Result<String, String> {
        let body = json!({
            "name": name,
            "platform": desktop_platform(),
            "kind": "DESKTOP",
            "publicKey": public_key,
        });
        let device: DeviceDto = self
            .authorized(Method::POST, "/devices", Some(&body))
            .await
            .map_err(|failure| match failure {
                Failure::Status(StatusCode::CONFLICT) => ERROR_DEVICE_CONFLICT.to_string(),
                other => failure_code(other),
            })?;
        Ok(device.id.to_string())
    }

    // The user's active devices (GET /devices). Used to recover this desktop's id when remote.toml
    // lost it but the key is still registered (POST /devices then answers 409).
    pub async fn devices(&self) -> Result<Vec<DeviceDto>, String> {
        self.authorized(Method::GET, "/devices", None)
            .await
            .map_err(failure_code)
    }

    // The registry entry of one of the user's devices; None when revoked or not theirs (404).
    pub async fn device(&self, id: &str) -> Result<Option<DeviceDto>, String> {
        let path = device_path(id)?;
        match self.authorized::<DeviceDto>(Method::GET, &path, None).await {
            Ok(device) => Ok(Some(device)),
            Err(Failure::Status(StatusCode::NOT_FOUND)) => Ok(None),
            Err(other) => Err(failure_code(other)),
        }
    }

    // Revoke a device on sito-auth. Already revoked (404) counts as done.
    pub async fn revoke_device(&self, id: &str) -> Result<(), String> {
        let path = device_path(id)?;
        let token = self.access_token().await?;
        match self.send(Method::DELETE, &path, Some(&token), None).await {
            Ok(_) | Err(Failure::Status(StatusCode::NOT_FOUND)) => Ok(()),
            Err(other) => Err(self.after_failure(other).await),
        }
    }

    async fn refresh(&self, state: &mut TokenState) -> Result<(), String> {
        let Some(refresh_token) = keyring_get(secrets::REFRESH_TOKEN).await? else {
            *state = TokenState::default();
            return Err(ERROR_SIGNED_OUT.to_string());
        };
        let body = json!({ "refreshToken": refresh_token });
        match self
            .request::<AuthDto>(Method::POST, "/auth/refresh", None, Some(&body))
            .await
        {
            Ok(dto) => self.accept_tokens(state, dto).await.map(|_| ()),
            // Rejected refresh token (expired, revoked, signed out everywhere): drop it so the
            // next attempt does not hit sito-auth again with a dead token.
            Err(Failure::Status(status)) if is_rejection(status) => {
                *state = TokenState::default();
                keyring_delete(secrets::REFRESH_TOKEN).await?;
                Err(ERROR_SIGNED_OUT.to_string())
            }
            Err(other) => Err(failure_code(other)),
        }
    }

    // Persist the rotated refresh token first, then cache the access token and account.
    async fn accept_tokens(&self, state: &mut TokenState, dto: AuthDto) -> Result<Account, String> {
        let (Some(token), Some(refresh_token)) = (dto.token, dto.refresh_token) else {
            return Err(ERROR_SERVER.to_string());
        };
        keyring_set(secrets::REFRESH_TOKEN, refresh_token).await?;
        let account = Account {
            user_id: dto.id,
            email: dto.email,
            username: dto.username,
        };
        state.access = Some(CachedAccess {
            refresh_at: refresh_deadline(token_exp(&token), now_unix()),
            token,
        });
        state.account = Some(account.clone());
        Ok(account)
    }

    async fn authorized<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, Failure> {
        let token = self.access_token().await.map_err(|code| {
            if code == ERROR_SIGNED_OUT {
                Failure::Status(StatusCode::UNAUTHORIZED)
            } else {
                Failure::Network
            }
        })?;
        match self.request(method, path, Some(&token), body).await {
            Err(Failure::Status(StatusCode::UNAUTHORIZED)) => {
                self.forget_access().await;
                Err(Failure::Status(StatusCode::UNAUTHORIZED))
            }
            other => other,
        }
    }

    // A 401 on an authorized call means the cached access token is no longer accepted; drop it so
    // the next call refreshes instead of reusing it.
    async fn after_failure(&self, failure: Failure) -> String {
        if matches!(failure, Failure::Status(StatusCode::UNAUTHORIZED)) {
            self.forget_access().await;
        }
        failure_code(failure)
    }

    // Also used by the relay client after a 401 handshake or a 4401 close.
    pub async fn forget_access(&self) {
        self.state.lock().await.access = None;
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<T, Failure> {
        let response = self.send(method, path, bearer, body).await?;
        response
            .json::<T>()
            .await
            .map_err(|_| Failure::Status(StatusCode::BAD_GATEWAY))
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<reqwest::Response, Failure> {
        let mut request = self
            .http
            .request(method, format!("{}{}", self.base_url, path));
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|_| Failure::Network)?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(Failure::Status(response.status()))
        }
    }
}

// sito-auth answers bad credentials / dead tokens with 400 (IllegalArgumentException), 401 or 403.
fn is_rejection(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    )
}

fn failure_code(failure: Failure) -> String {
    match failure {
        Failure::Network => ERROR_NETWORK,
        Failure::Status(StatusCode::UNAUTHORIZED) => ERROR_SIGNED_OUT,
        Failure::Status(StatusCode::TOO_MANY_REQUESTS) => ERROR_RATE_LIMITED,
        Failure::Status(_) => ERROR_SERVER,
    }
    .to_string()
}

// Device ids are numeric; refusing anything else keeps a hostile id from reshaping the URL path.
fn device_path(id: &str) -> Result<String, String> {
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ERROR_SERVER.to_string());
    }
    Ok(format!("/devices/{id}"))
}

fn desktop_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "MACOS"
    } else if cfg!(target_os = "windows") {
        "WINDOWS"
    } else {
        "LINUX"
    }
}

// The access token's `exp` claim, read without verifying the signature: it only schedules the
// next refresh. Trust decisions about the token are sito-auth's and the relay's.
pub(crate) fn token_exp(token: &str) -> Option<u64> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<JwtClaims>(&bytes)
        .ok()
        .map(|claims| claims.exp)
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn refresh_deadline(exp: Option<u64>, now: u64) -> Instant {
    let lifetime = match exp {
        Some(exp) => Duration::from_secs(exp.saturating_sub(now)),
        None => ACCESS_FALLBACK_LIFETIME,
    };
    Instant::now() + lifetime.saturating_sub(ACCESS_REFRESH_MARGIN)
}

// Keyring calls block (and may prompt on macOS): always on the blocking pool.
async fn keyring_get(name: &'static str) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || secrets::get(name))
        .await
        .map_err(|e| e.to_string())?
}

async fn keyring_set(name: &'static str, secret: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || secrets::set(name, &secret))
        .await
        .map_err(|e| e.to_string())?
}

async fn keyring_delete(name: &'static str) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || secrets::delete(name))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_token(claims: &str) -> String {
        format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#),
            URL_SAFE_NO_PAD.encode(claims)
        )
    }

    #[test]
    fn reads_exp_claim() {
        let token = fake_token(r#"{"sub":"7","exp":1700000900,"type":"access"}"#);
        assert_eq!(token_exp(&token), Some(1_700_000_900));
        assert_eq!(token_exp("not-a-jwt"), None);
        assert_eq!(token_exp(&fake_token(r#"{"sub":"7"}"#)), None);
    }

    #[test]
    fn refreshes_before_expiry() {
        let now = 1_700_000_000;
        let deadline = refresh_deadline(Some(now + 900), now);
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(remaining <= Duration::from_secs(840));
        assert!(remaining > Duration::from_secs(830));
        // Already expired: refresh immediately instead of underflowing.
        let expired = refresh_deadline(Some(now - 10), now);
        assert!(expired <= Instant::now());
    }

    #[test]
    fn device_path_rejects_non_numeric_ids() {
        assert_eq!(device_path("42"), Ok("/devices/42".to_string()));
        for bad in ["", "../users", "4/2", "42?x=1"] {
            assert!(device_path(bad).is_err(), "{bad}");
        }
    }
}

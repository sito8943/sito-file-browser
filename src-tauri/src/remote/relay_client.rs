use std::sync::Arc;
use std::time::{Duration, SystemTime};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::time::{interval_at, sleep, sleep_until, timeout, Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{header::AUTHORIZATION, HeaderValue, StatusCode};
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use super::auth::{now_unix, token_exp, AuthClient, ACCESS_REFRESH_MARGIN, ERROR_SIGNED_OUT};

// Relay endpoint. Wire contract: servers/sito-remote-relay/readme.md.
pub const RELAY_URL: &str = "wss://api.remote.sito8943.com/ws";
// Lowercase on purpose: `http` header names built from static strs must be lowercase.
const DEVICE_HEADER: &str = "x-device-id";

// The relay drops idle sockets after 90 s and asks clients to ping every 25 s.
const PING_INTERVAL: Duration = Duration::from_secs(25);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
// The relay closes the socket (4401) at the token's `exp` unless it gets `auth` with a fresh one.
// Renew this long before `exp`. It must be shorter than the auth client's refresh margin, or
// access_token() would still hand back the cached (about to expire) token.
const RENEW_BEFORE_EXP: Duration = Duration::from_secs(45);
const _: () = assert!(RENEW_BEFORE_EXP.as_secs() < ACCESS_REFRESH_MARGIN.as_secs());
// When `exp` is unknown, or a renewal failed transiently.
const RENEW_FALLBACK: Duration = Duration::from_secs(10 * 60);
const RENEW_RETRY: Duration = Duration::from_secs(15);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
// Two 401s in a row, even with a freshly refreshed token, mean the device itself is refused
// (revoked or never registered), not an expired token.
const MAX_UNAUTHORIZED_STREAK: u32 = 2;
const OUTBOUND_QUEUE: usize = 64;

const CLOSE_REPLACED: u16 = 4000;
const CLOSE_UNAUTHORIZED: u16 = 4401;

const ERROR_QUEUE_FULL: &str = "REMOTE_RELAY_BUSY";
const ERROR_STOPPED: &str = "REMOTE_RELAY_STOPPED";

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

// A device of the same user as the relay reports it. Ids are numeric on the wire.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PresenceDevice {
    pub id: u64,
    pub name: String,
    pub kind: String,
}

// A routed message for this desktop. `payload` is the base64 ciphertext; this module never
// interprets it (decryption and the protocol live above, in the session layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteIn {
    pub from: u64,
    pub sid: String,
    pub seq: u64,
    pub payload: String,
}

// Why the client stopped for good (no more reconnects until the service restarts it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    // No sito-auth session (signed out, refresh token revoked).
    SignedOut,
    // Another connection for this same device took over (4000). Reconnecting would just
    // kick it out in turn, so the two would fight forever.
    Replaced,
    // The relay refuses this device even with a fresh token: revoked or not registered.
    Rejected,
    // The service asked to stop (setting turned off, app quitting).
    Requested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayEvent {
    Connected,
    Disconnected,
    Presence(Vec<PresenceDevice>),
    PresenceUpdate { device: PresenceDevice, online: bool },
    Route(RouteIn),
    Error { code: String, reference: Option<u64> },
    Stopped(StopReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outbound {
    Route {
        to: u64,
        sid: String,
        seq: u64,
        payload: String,
    },
    Presence,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "t")]
enum Incoming {
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "presence")]
    Presence { devices: Vec<PresenceDevice> },
    #[serde(rename = "presence.update")]
    PresenceUpdate { device: PresenceDevice, online: bool },
    #[serde(rename = "route")]
    Route {
        from: u64,
        sid: String,
        seq: u64,
        p: String,
    },
    #[serde(rename = "auth.ok")]
    AuthOk { exp: u64 },
    #[serde(rename = "error")]
    Error {
        code: String,
        #[serde(rename = "ref", default)]
        reference: Option<u64>,
    },
}

#[derive(Serialize)]
#[serde(tag = "t")]
enum Frame<'a> {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "presence")]
    Presence,
    #[serde(rename = "route")]
    Route {
        to: u64,
        sid: &'a str,
        seq: u64,
        p: &'a str,
    },
    #[serde(rename = "auth")]
    Auth { token: &'a str },
}

enum ConnectError {
    SignedOut,
    Unauthorized,
    Transient,
}

enum SessionEnd {
    Reconnect,
    Unauthorized,
    Stop(StopReason),
}

// Handle owned by the remote service. Dropping it stops the client.
pub struct RelayHandle {
    outbound: mpsc::Sender<Outbound>,
    stop: watch::Sender<bool>,
}

impl RelayHandle {
    // Queue a frame. Never waits: a full queue means the link is stalled, and the caller (the
    // session layer) answers its phone with RATE_LIMITED rather than piling up responses.
    pub fn send(&self, message: Outbound) -> Result<(), String> {
        self.outbound.try_send(message).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => ERROR_QUEUE_FULL.to_string(),
            mpsc::error::TrySendError::Closed(_) => ERROR_STOPPED.to_string(),
        })
    }

    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

impl Drop for RelayHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

// Start the relay connection loop on the async runtime (never Tauri's main thread). Events,
// including the final Stopped, arrive on `events`.
pub fn spawn(
    auth: Arc<AuthClient>,
    device_id: u64,
    events: mpsc::Sender<RelayEvent>,
) -> RelayHandle {
    // Explicit TLS provider: rustls refuses to pick one when several crates enable different
    // backends. Err just means it is already installed.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let (outbound, outbound_rx) = mpsc::channel(OUTBOUND_QUEUE);
    let (stop, stop_rx) = watch::channel(false);
    tauri::async_runtime::spawn(run(auth, device_id, events, outbound_rx, stop_rx));
    RelayHandle { outbound, stop }
}

async fn run(
    auth: Arc<AuthClient>,
    device_id: u64,
    events: mpsc::Sender<RelayEvent>,
    mut outbound_rx: mpsc::Receiver<Outbound>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut backoff = BACKOFF_MIN;
    let mut unauthorized_streak = 0;

    let reason = loop {
        if *stop_rx.borrow() {
            break StopReason::Requested;
        }
        let end = match connect(&auth, device_id).await {
            Ok((socket, exp)) => {
                backoff = BACKOFF_MIN;
                unauthorized_streak = 0;
                let _ = events.send(RelayEvent::Connected).await;
                let end = session(
                    socket,
                    exp,
                    &auth,
                    &events,
                    &mut outbound_rx,
                    &mut stop_rx,
                )
                .await;
                let _ = events.send(RelayEvent::Disconnected).await;
                end
            }
            Err(ConnectError::SignedOut) => SessionEnd::Stop(StopReason::SignedOut),
            Err(ConnectError::Unauthorized) => SessionEnd::Unauthorized,
            Err(ConnectError::Transient) => SessionEnd::Reconnect,
        };

        match end {
            SessionEnd::Stop(reason) => break reason,
            // Drop the cached token and retry at once: the next connect refreshes it. A second
            // refusal with that fresh token means the device is refused.
            SessionEnd::Unauthorized => {
                auth.forget_access().await;
                unauthorized_streak += 1;
                if unauthorized_streak >= MAX_UNAUTHORIZED_STREAK {
                    break StopReason::Rejected;
                }
            }
            SessionEnd::Reconnect => {
                if wait_backoff(with_jitter(backoff), &mut stop_rx, &mut outbound_rx).await {
                    break StopReason::Requested;
                }
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    };
    let _ = events.send(RelayEvent::Stopped(reason)).await;
}

async fn connect(auth: &AuthClient, device_id: u64) -> Result<(Socket, Option<u64>), ConnectError> {
    let token = match auth.access_token().await {
        Ok(token) => token,
        Err(code) if code == ERROR_SIGNED_OUT => return Err(ConnectError::SignedOut),
        Err(_) => return Err(ConnectError::Transient),
    };
    let mut request = RELAY_URL
        .into_client_request()
        .map_err(|_| ConnectError::Transient)?;
    let headers = request.headers_mut();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| ConnectError::Transient)?,
    );
    headers.insert(
        DEVICE_HEADER,
        HeaderValue::from_str(&device_id.to_string()).map_err(|_| ConnectError::Transient)?,
    );

    match timeout(CONNECT_TIMEOUT, connect_async(request)).await {
        Ok(Ok((socket, _response))) => Ok((socket, token_exp(&token))),
        Ok(Err(tungstenite::Error::Http(response)))
            if response.status() == StatusCode::UNAUTHORIZED =>
        {
            Err(ConnectError::Unauthorized)
        }
        // 503 (sito-auth down), DNS, TLS, timeouts: all worth retrying with backoff.
        _ => Err(ConnectError::Transient),
    }
}

async fn session(
    socket: Socket,
    exp: Option<u64>,
    auth: &AuthClient,
    events: &mpsc::Sender<RelayEvent>,
    outbound_rx: &mut mpsc::Receiver<Outbound>,
    stop_rx: &mut watch::Receiver<bool>,
) -> SessionEnd {
    let (mut sink, mut stream) = socket.split();
    let mut ping = interval_at(Instant::now() + PING_INTERVAL, PING_INTERVAL);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut renew_at = renew_deadline(exp);

    // Learn which phones are already online; later changes arrive as presence.update.
    if send_frame(&mut sink, &Frame::Presence).await.is_err() {
        return SessionEnd::Reconnect;
    }

    loop {
        tokio::select! {
            _ = stop_rx.changed() => {
                let _ = sink.send(Message::Close(None)).await;
                return SessionEnd::Stop(StopReason::Requested);
            }
            _ = ping.tick() => {
                if send_frame(&mut sink, &Frame::Ping).await.is_err() {
                    return SessionEnd::Reconnect;
                }
            }
            _ = sleep_until(renew_at) => {
                match auth.access_token().await {
                    Ok(token) => {
                        if send_frame(&mut sink, &Frame::Auth { token: &token }).await.is_err() {
                            return SessionEnd::Reconnect;
                        }
                        // auth.ok moves this to the new exp; the fallback only guards against
                        // a lost reply.
                        renew_at = Instant::now() + RENEW_FALLBACK;
                    }
                    Err(code) if code == ERROR_SIGNED_OUT => {
                        let _ = sink.send(Message::Close(None)).await;
                        return SessionEnd::Stop(StopReason::SignedOut);
                    }
                    Err(_) => renew_at = Instant::now() + RENEW_RETRY,
                }
            }
            message = outbound_rx.recv() => {
                let Some(message) = message else {
                    return SessionEnd::Stop(StopReason::Requested);
                };
                let sent = match &message {
                    Outbound::Route { to, sid, seq, payload } => {
                        let frame = Frame::Route { to: *to, sid, seq: *seq, p: payload };
                        send_frame(&mut sink, &frame).await
                    }
                    Outbound::Presence => send_frame(&mut sink, &Frame::Presence).await,
                };
                if sent.is_err() {
                    return SessionEnd::Reconnect;
                }
            }
            incoming = stream.next() => {
                let message = match incoming {
                    Some(Ok(message)) => message,
                    Some(Err(_)) | None => return SessionEnd::Reconnect,
                };
                match message {
                    Message::Text(text) => {
                        if let Some(next) = on_text(text.as_str(), events).await {
                            renew_at = next;
                        }
                    }
                    Message::Close(frame) => {
                        return match frame.map(|frame| u16::from(frame.code)) {
                            Some(CLOSE_REPLACED) => SessionEnd::Stop(StopReason::Replaced),
                            Some(CLOSE_UNAUTHORIZED) => SessionEnd::Unauthorized,
                            _ => SessionEnd::Reconnect,
                        };
                    }
                    // WebSocket-level ping/pong is answered by tungstenite; the relay never
                    // sends binary frames.
                    _ => {}
                }
            }
        }
    }
}

// Handle one relay text frame. Returns a new renewal deadline on auth.ok. Unknown or malformed
// frames are ignored so a newer relay can add message types without breaking older desktops.
async fn on_text(text: &str, events: &mpsc::Sender<RelayEvent>) -> Option<Instant> {
    let event = match serde_json::from_str::<Incoming>(text).ok()? {
        Incoming::Pong => return None,
        Incoming::AuthOk { exp } => return Some(renew_deadline(Some(exp))),
        Incoming::Presence { devices } => RelayEvent::Presence(devices),
        Incoming::PresenceUpdate { device, online } => {
            RelayEvent::PresenceUpdate { device, online }
        }
        Incoming::Route { from, sid, seq, p } => RelayEvent::Route(RouteIn {
            from,
            sid,
            seq,
            payload: p,
        }),
        Incoming::Error { code, reference } => RelayEvent::Error { code, reference },
    };
    let _ = events.send(event).await;
    None
}

async fn send_frame<S>(sink: &mut S, frame: &Frame<'_>) -> Result<(), ()>
where
    S: SinkExt<Message> + Unpin,
{
    let text = serde_json::to_string(frame).map_err(|_| ())?;
    sink.send(Message::text(text)).await.map_err(|_| ())
}

fn renew_deadline(exp: Option<u64>) -> Instant {
    let lifetime = match exp {
        Some(exp) => Duration::from_secs(exp.saturating_sub(now_unix())),
        None => RENEW_FALLBACK,
    };
    Instant::now() + lifetime.saturating_sub(RENEW_BEFORE_EXP)
}

// Up to +25% so many desktops dropped by one relay restart don't reconnect in lockstep. The clock's
// sub-second part is random enough for spreading retries; this is not security-relevant.
fn with_jitter(delay: Duration) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0);
    delay + delay.mul_f64(f64::from(nanos % 250) / 1000.0)
}

// Wait out a reconnect delay. Frames queued while offline are dropped: they answer requests of a
// session that died with the socket. Returns true when asked to stop.
async fn wait_backoff(
    delay: Duration,
    stop_rx: &mut watch::Receiver<bool>,
    outbound_rx: &mut mpsc::Receiver<Outbound>,
) -> bool {
    let deadline = sleep(delay);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => return false,
            _ = stop_rx.changed() => return true,
            message = outbound_rx.recv() => {
                if message.is_none() {
                    return true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relay_frames() {
        let route: Incoming =
            serde_json::from_str(r#"{"t":"route","from":12,"sid":"s1","seq":3,"p":"QUJD"}"#)
                .unwrap();
        assert_eq!(
            route,
            Incoming::Route {
                from: 12,
                sid: "s1".to_string(),
                seq: 3,
                p: "QUJD".to_string()
            }
        );
        let update: Incoming = serde_json::from_str(
            r#"{"t":"presence.update","device":{"id":7,"name":"Pixel","kind":"MOBILE"},"online":true}"#,
        )
        .unwrap();
        assert!(matches!(update, Incoming::PresenceUpdate { online: true, .. }));
        let error: Incoming =
            serde_json::from_str(r#"{"t":"error","code":"NOT_ONLINE","ref":9}"#).unwrap();
        assert_eq!(
            error,
            Incoming::Error {
                code: "NOT_ONLINE".to_string(),
                reference: Some(9)
            }
        );
        let without_ref: Incoming =
            serde_json::from_str(r#"{"t":"error","code":"BAD_MESSAGE"}"#).unwrap();
        assert!(matches!(without_ref, Incoming::Error { reference: None, .. }));
        assert!(serde_json::from_str::<Incoming>(r#"{"t":"future.thing"}"#).is_err());
    }

    #[test]
    fn serializes_client_frames() {
        let route = Frame::Route {
            to: 7,
            sid: "s1",
            seq: 0,
            p: "QUJD",
        };
        assert_eq!(
            serde_json::to_string(&route).unwrap(),
            r#"{"t":"route","to":7,"sid":"s1","seq":0,"p":"QUJD"}"#
        );
        assert_eq!(serde_json::to_string(&Frame::Ping).unwrap(), r#"{"t":"ping"}"#);
        assert_eq!(
            serde_json::to_string(&Frame::Auth { token: "x" }).unwrap(),
            r#"{"t":"auth","token":"x"}"#
        );
    }

    #[test]
    fn renews_before_exp() {
        let remaining =
            renew_deadline(Some(now_unix() + 900)).saturating_duration_since(Instant::now());
        assert!(remaining <= Duration::from_secs(855));
        assert!(remaining > Duration::from_secs(845));
        assert!(renew_deadline(Some(0)) <= Instant::now());
    }

    #[test]
    fn jitter_stays_within_a_quarter() {
        let delay = Duration::from_secs(4);
        let jittered = with_jitter(delay);
        assert!(jittered >= delay && jittered <= delay + Duration::from_secs(1));
    }
}

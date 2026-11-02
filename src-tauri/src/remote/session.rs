//! Session engine: turns relay `route` messages into handshakes, decrypted requests and sealed
//! responses. Pure state machine (no network, no Tauri, no disk) so the whole flow is testable by
//! simulating the phone with crypto.rs. The service loop (remote/mod.rs) feeds it relay events,
//! performs the returned actions, and does the async work it cannot (registry lookup, handlers).
//!
//! Protocol: plans/REMOTE_CONTROL_PLAN.md, "Protocolo E2E v1".

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::Deserialize;
use serde_json::json;

use super::crypto::{self, Channel, Handshake, KeyPair, Mode, Role, CONFIRM, POINT_LEN};
use super::error::RemoteError;
use super::handlers::{Request, Response};
use super::pairing::{Pairing, CODE_LEN};
use super::relay_client::{Outbound, RouteIn};
use super::store::RemoteConfig;
use super::wire::{self, RejectCode, WireError, WireFrame};

// Concurrent requests per phone. Above it the desktop answers RATE_LIMITED instead of queueing
// more disk work (each request runs on the blocking pool).
pub const MAX_IN_FLIGHT: usize = 8;

const OP_PAIR_HELLO: &str = "pair.hello";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Hand to the relay client.
    Send(Outbound),
    // PAIR_HELLO with a live code: the loop checks the sito-auth registry (GET /devices/{from}:
    // exists, MOBILE, publicKey == static_spki) and then calls accept_pairing or reject.
    VerifyPairing {
        from: u64,
        sid: String,
        ephemeral: [u8; POINT_LEN],
        static_spki: Vec<u8>,
        code: [u8; CODE_LEN],
    },
    // A decrypted, allowlisted request to run (handlers::handle) and answer with respond().
    Serve {
        from: u64,
        sid: String,
        id: u64,
        request: Request,
    },
    // The phone proved the pairing code; show the SAS and wait for the user (Pairing::set_pending).
    PairingProved {
        device_id: u64,
        name: String,
        public_key: String,
        sas: String,
    },
    // A serving session started (toast / active-session indicator).
    SessionOpened { device_id: u64 },
    // A session ended: protocol error, replaced, pairing finished, or the device was dropped.
    SessionClosed { device_id: u64 },
}

enum Stage {
    Serving { in_flight: usize },
    // PAIR_ACK sent; waiting for the phone's first sealed message.
    PairingAwaitProof { name: String, public_key: String },
    // SAS shown; waiting for the user to confirm on the desktop.
    PairingAwaitUser,
}

struct Live {
    sid: String,
    channel: Channel,
    stage: Stage,
}

#[derive(Deserialize)]
struct RequestId {
    id: u64,
}

#[derive(Deserialize)]
struct PairProof {
    op: String,
}

pub struct SessionEngine {
    identity: Arc<KeyPair>,
    sessions: HashMap<u64, Live>,
}

impl SessionEngine {
    pub fn new(identity: Arc<KeyPair>) -> Self {
        Self {
            identity,
            sessions: HashMap::new(),
        }
    }

    // Phones with an established serving session.
    pub fn active_devices(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, live)| matches!(live.stage, Stage::Serving { .. }))
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub fn on_route(
        &mut self,
        route: RouteIn,
        config: &RemoteConfig,
        pairing: &mut Pairing,
        now: Instant,
    ) -> Vec<Action> {
        let frame = match wire::decode(&route.payload) {
            Ok(frame) => frame,
            Err(WireError::UnsupportedVersion) => {
                return vec![reject(route.from, &route.sid, RejectCode::UnsupportedVersion)]
            }
            // Garbage is not worth an answer; it may not even come from a real phone.
            Err(WireError::Malformed) => return Vec::new(),
        };
        match frame {
            WireFrame::Hello { ephemeral } => self.on_hello(route, ephemeral, config),
            WireFrame::PairHello {
                ephemeral,
                static_spki,
            } => match pairing.take_code(now) {
                Some(code) => vec![Action::VerifyPairing {
                    from: route.from,
                    sid: route.sid,
                    ephemeral,
                    static_spki,
                    code,
                }],
                None => vec![reject(route.from, &route.sid, RejectCode::PairingClosed)],
            },
            WireFrame::Data { ciphertext } => self.on_data(route, &ciphertext),
            // Desktop-to-phone frames never arrive at a desktop.
            WireFrame::HelloAck { .. } | WireFrame::PairAck { .. } | WireFrame::Reject { .. } => {
                Vec::new()
            }
        }
    }

    // The registry confirmed the phone; finish the pairing handshake. `name` comes from the
    // registry, not from the phone.
    pub fn accept_pairing(
        &mut self,
        from: u64,
        sid: &str,
        ephemeral: [u8; POINT_LEN],
        static_spki: &[u8],
        code: &[u8; CODE_LEN],
        name: String,
    ) -> Vec<Action> {
        let Ok(mobile_static) = crypto::spki_to_point(static_spki) else {
            return vec![reject(from, sid, RejectCode::DeviceMismatch)];
        };
        let mut actions = self.close(from);
        match self.handshake(Mode::Pairing, sid, mobile_static, ephemeral, Some(code)) {
            Ok((ack_ephemeral, mut channel)) => match channel.seal(CONFIRM) {
                Ok((seq, confirm)) => {
                    let frame = WireFrame::PairAck {
                        ephemeral: ack_ephemeral,
                        confirm,
                    };
                    actions.push(send(from, sid, seq, &frame));
                    self.sessions.insert(
                        from,
                        Live {
                            sid: sid.to_string(),
                            channel,
                            stage: Stage::PairingAwaitProof {
                                name,
                                public_key: BASE64.encode(static_spki),
                            },
                        },
                    );
                }
                Err(_) => actions.push(reject(from, sid, RejectCode::Busy)),
            },
            Err(_) => actions.push(reject(from, sid, RejectCode::DeviceMismatch)),
        }
        actions
    }

    // The user answered the SAS prompt. The caller persists the device first when accepted.
    pub fn finish_pairing(&mut self, device_id: u64, accepted: bool) -> Vec<Action> {
        let Some(mut live) = self.sessions.remove(&device_id) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        if matches!(live.stage, Stage::PairingAwaitUser) {
            let message = if accepted {
                json!({ "t": "pair.ok" })
            } else {
                json!({ "t": "pair.rejected" })
            };
            if let Ok((seq, ciphertext)) = live.channel.seal(message.to_string().as_bytes()) {
                actions.push(send(
                    device_id,
                    &live.sid,
                    seq,
                    &WireFrame::Data { ciphertext },
                ));
            }
        }
        actions.push(Action::SessionClosed { device_id });
        actions
    }

    // Seal a handler result. Dropped if the session was replaced or closed meanwhile.
    pub fn respond(
        &mut self,
        from: u64,
        sid: &str,
        id: u64,
        result: Result<Response, RemoteError>,
    ) -> Vec<Action> {
        let Some(live) = self.sessions.get_mut(&from) else {
            return Vec::new();
        };
        if live.sid != sid {
            return Vec::new();
        }
        if let Stage::Serving { in_flight } = &mut live.stage {
            *in_flight = in_flight.saturating_sub(1);
        }
        self.reply(from, id, result)
    }

    // Revocation or service stop: forget the session at once (no farewell message).
    pub fn close(&mut self, device_id: u64) -> Vec<Action> {
        match self.sessions.remove(&device_id) {
            Some(_) => vec![Action::SessionClosed { device_id }],
            None => Vec::new(),
        }
    }

    fn on_hello(
        &mut self,
        route: RouteIn,
        ephemeral: [u8; POINT_LEN],
        config: &RemoteConfig,
    ) -> Vec<Action> {
        // Only keys this desktop paired itself are served (the trust boundary).
        let mobile_static = config
            .device(&route.from.to_string())
            .and_then(|device| BASE64.decode(&device.public_key).ok())
            .and_then(|spki| crypto::spki_to_point(&spki).ok());
        let Some(mobile_static) = mobile_static else {
            return vec![reject(route.from, &route.sid, RejectCode::NotPaired)];
        };
        // A new hello replaces the previous session of that phone.
        let mut actions = self.close(route.from);
        match self.handshake(Mode::Session, &route.sid, mobile_static, ephemeral, None) {
            Ok((ack_ephemeral, mut channel)) => match channel.seal(CONFIRM) {
                Ok((seq, confirm)) => {
                    let frame = WireFrame::HelloAck {
                        ephemeral: ack_ephemeral,
                        confirm,
                    };
                    actions.push(send(route.from, &route.sid, seq, &frame));
                    self.sessions.insert(
                        route.from,
                        Live {
                            sid: route.sid,
                            channel,
                            stage: Stage::Serving { in_flight: 0 },
                        },
                    );
                    actions.push(Action::SessionOpened {
                        device_id: route.from,
                    });
                }
                Err(_) => actions.push(reject(route.from, &route.sid, RejectCode::Busy)),
            },
            // Invalid ephemeral point.
            Err(_) => {}
        }
        actions
    }

    fn on_data(&mut self, route: RouteIn, ciphertext: &[u8]) -> Vec<Action> {
        let from = route.from;
        let Some(live) = self.sessions.get_mut(&from) else {
            return Vec::new();
        };
        // Data for an older session of this phone (it re-helloed meanwhile).
        if live.sid != route.sid {
            return Vec::new();
        }
        // Wrong key, tampering, replay or a gap: the session cannot be trusted any more.
        let Ok(plaintext) = live.channel.open(route.seq, ciphertext) else {
            return self.close(from);
        };

        match &mut live.stage {
            Stage::Serving { in_flight } => {
                let Ok(RequestId { id }) = serde_json::from_slice::<RequestId>(&plaintext) else {
                    return self.close(from);
                };
                if *in_flight >= MAX_IN_FLIGHT {
                    return self.reply(from, id, Err(RemoteError::RateLimited));
                }
                match serde_json::from_slice::<Request>(&plaintext) {
                    Ok(request) => {
                        *in_flight += 1;
                        vec![Action::Serve {
                            from,
                            sid: route.sid,
                            id,
                            request,
                        }]
                    }
                    // Not in the allowlist (a write verb, an unknown op, missing fields).
                    Err(_) => self.reply(from, id, Err(RemoteError::Forbidden)),
                }
            }
            Stage::PairingAwaitProof { name, public_key } => {
                let proved = serde_json::from_slice::<PairProof>(&plaintext)
                    .map(|proof| proof.op == OP_PAIR_HELLO)
                    .unwrap_or(false);
                if !proved {
                    return self.close(from);
                }
                let action = Action::PairingProved {
                    device_id: from,
                    name: std::mem::take(name),
                    public_key: std::mem::take(public_key),
                    sas: live.channel.sas(),
                };
                live.stage = Stage::PairingAwaitUser;
                vec![action]
            }
            // Nothing is accepted from the phone until the user decides.
            Stage::PairingAwaitUser => self.close(from),
        }
    }

    fn reply(&mut self, from: u64, id: u64, result: Result<Response, RemoteError>) -> Vec<Action> {
        let Some(live) = self.sessions.get_mut(&from) else {
            return Vec::new();
        };
        let body = match result {
            Ok(response) => json!({ "id": id, "ok": response }),
            Err(code) => json!({ "id": id, "error": code }),
        };
        match live.channel.seal(body.to_string().as_bytes()) {
            Ok((seq, ciphertext)) => {
                let sid = live.sid.clone();
                vec![send(from, &sid, seq, &WireFrame::Data { ciphertext })]
            }
            Err(_) => self.close(from),
        }
    }

    fn handshake(
        &self,
        mode: Mode,
        sid: &str,
        mobile_static: [u8; POINT_LEN],
        mobile_ephemeral: [u8; POINT_LEN],
        psk: Option<&[u8; CODE_LEN]>,
    ) -> Result<([u8; POINT_LEN], Channel), crypto::CryptoError> {
        let ephemeral = KeyPair::generate()?;
        let handshake = Handshake {
            mode,
            sid,
            desktop_static: *self.identity.point(),
            mobile_static,
            desktop_ephemeral: *ephemeral.point(),
            mobile_ephemeral,
            psk: psk.map(|code| &code[..]),
        };
        let channel = crypto::derive(Role::Desktop, &self.identity, &ephemeral, &handshake)?;
        Ok((*ephemeral.point(), channel))
    }
}

fn send(to: u64, sid: &str, seq: u64, frame: &WireFrame) -> Action {
    Action::Send(Outbound::Route {
        to,
        sid: sid.to_string(),
        seq,
        payload: wire::encode(frame),
    })
}

// REJECT travels outside any channel, so it carries no meaningful seq.
pub fn reject(to: u64, sid: &str, code: RejectCode) -> Action {
    send(to, sid, 0, &WireFrame::Reject { code: code as u8 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::store::{PairedDevice, RemoteRoot};

    const PHONE: u64 = 42;
    const SID: &str = "0123456789abcdef0123456789abcdef";

    // Simulated phone: its static key and, once handshaken, its channel.
    struct Phone {
        static_key: KeyPair,
        channel: Option<Channel>,
        next_seq: u64,
    }

    impl Phone {
        fn new() -> Self {
            Self {
                static_key: KeyPair::generate().unwrap(),
                channel: None,
                next_seq: 0,
            }
        }

        fn route(&mut self, frame: &WireFrame) -> RouteIn {
            let seq = self.next_seq;
            self.next_seq += 1;
            RouteIn {
                from: PHONE,
                sid: SID.to_string(),
                seq,
                payload: wire::encode(frame),
            }
        }

        fn data(&mut self, json: &str) -> RouteIn {
            let channel = self.channel.as_mut().unwrap();
            let (seq, ciphertext) = channel.seal(json.as_bytes()).unwrap();
            assert_eq!(seq, self.next_seq);
            self.route(&WireFrame::Data { ciphertext })
        }

        // Complete the handshake from the desktop's ACK, verifying CONFIRM.
        fn finish(
            &mut self,
            mode: Mode,
            ephemeral: &KeyPair,
            desktop: &KeyPair,
            action: &Action,
            psk: Option<&[u8]>,
        ) {
            let frame = decode_sent(action);
            let (desktop_ephemeral, confirm) = match frame {
                WireFrame::HelloAck { ephemeral, confirm }
                | WireFrame::PairAck { ephemeral, confirm } => (ephemeral, confirm),
                other => panic!("expected ack, got {other:?}"),
            };
            let handshake = Handshake {
                mode,
                sid: SID,
                desktop_static: *desktop.point(),
                mobile_static: *self.static_key.point(),
                desktop_ephemeral,
                mobile_ephemeral: *ephemeral.point(),
                psk,
            };
            let mut channel =
                crypto::derive(Role::Mobile, &self.static_key, ephemeral, &handshake).unwrap();
            assert_eq!(channel.open(0, &confirm).unwrap(), CONFIRM);
            self.channel = Some(channel);
        }

        fn open(&mut self, action: &Action) -> serde_json::Value {
            let Action::Send(Outbound::Route { seq, .. }) = action else {
                panic!("expected send, got {action:?}");
            };
            let WireFrame::Data { ciphertext } = decode_sent(action) else {
                panic!("expected data");
            };
            let plaintext = self
                .channel
                .as_mut()
                .unwrap()
                .open(*seq, &ciphertext)
                .unwrap();
            serde_json::from_slice(&plaintext).unwrap()
        }
    }

    fn decode_sent(action: &Action) -> WireFrame {
        match action {
            Action::Send(Outbound::Route { to, sid, payload, .. }) => {
                assert_eq!((*to, sid.as_str()), (PHONE, SID));
                wire::decode(payload).unwrap()
            }
            other => panic!("expected send, got {other:?}"),
        }
    }

    fn config_with(phone: &Phone) -> RemoteConfig {
        RemoteConfig {
            desktop_device_id: Some("1".to_string()),
            roots: vec![RemoteRoot {
                id: "r1".to_string(),
                name: "Photos".to_string(),
                path: "/nonexistent".to_string(),
            }],
            devices: vec![PairedDevice {
                id: PHONE.to_string(),
                name: "Pixel".to_string(),
                platform: "ANDROID".to_string(),
                public_key: BASE64.encode(phone.static_key.spki()),
                paired_at: 0,
            }],
        }
    }

    fn engine() -> (SessionEngine, Arc<KeyPair>) {
        let desktop = Arc::new(KeyPair::generate().unwrap());
        (SessionEngine::new(desktop.clone()), desktop)
    }

    #[test]
    fn paired_phone_gets_a_session_and_answers() {
        let (mut engine, desktop) = engine();
        let mut phone = Phone::new();
        let config = config_with(&phone);
        let mut pairing = Pairing::default();
        let ephemeral = KeyPair::generate().unwrap();

        let hello = phone.route(&WireFrame::Hello {
            ephemeral: *ephemeral.point(),
        });
        let actions = engine.on_route(hello, &config, &mut pairing, Instant::now());
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[1], Action::SessionOpened { device_id: PHONE });
        phone.finish(Mode::Session, &ephemeral, &desktop, &actions[0], None);
        assert_eq!(engine.active_devices(), vec![PHONE]);

        let request = phone.data(r#"{"id":1,"op":"roots.list"}"#);
        let actions = engine.on_route(request, &config, &mut pairing, Instant::now());
        let [Action::Serve { id, request, sid, .. }] = actions.as_slice() else {
            panic!("expected serve, got {actions:?}");
        };
        assert_eq!((*id, request), (1, &Request::RootsList));

        let response = Response::Roots { roots: Vec::new() };
        let sid = sid.clone();
        let actions = engine.respond(PHONE, &sid, 1, Ok(response));
        let body = phone.open(&actions[0]);
        assert_eq!(body["id"], 1);
        assert_eq!(body["ok"]["kind"], "roots");
    }

    #[test]
    fn write_ops_are_forbidden_inside_the_channel() {
        let (mut engine, desktop) = engine();
        let mut phone = Phone::new();
        let config = config_with(&phone);
        let mut pairing = Pairing::default();
        let ephemeral = KeyPair::generate().unwrap();
        let hello = phone.route(&WireFrame::Hello {
            ephemeral: *ephemeral.point(),
        });
        let actions = engine.on_route(hello, &config, &mut pairing, Instant::now());
        phone.finish(Mode::Session, &ephemeral, &desktop, &actions[0], None);

        let delete = phone.data(r#"{"id":7,"op":"file.delete","root":"r1","path":"a"}"#);
        let actions = engine.on_route(delete, &config, &mut pairing, Instant::now());
        let body = phone.open(&actions[0]);
        assert_eq!(body["id"], 7);
        assert_eq!(body["error"], "FORBIDDEN");
    }

    #[test]
    fn unpaired_phone_is_rejected() {
        let (mut engine, _desktop) = engine();
        let mut phone = Phone::new();
        let stranger = Phone::new();
        let config = config_with(&stranger);
        let mut config_without = config.clone();
        config_without.devices.clear();
        let ephemeral = KeyPair::generate().unwrap();

        let hello = phone.route(&WireFrame::Hello {
            ephemeral: *ephemeral.point(),
        });
        let actions = engine.on_route(
            hello,
            &config_without,
            &mut Pairing::default(),
            Instant::now(),
        );
        assert_eq!(
            decode_sent(&actions[0]),
            WireFrame::Reject {
                code: RejectCode::NotPaired as u8
            }
        );
        assert!(engine.active_devices().is_empty());
    }

    #[test]
    fn phone_with_another_key_cannot_talk() {
        // The relay (or a thief of the account) reuses a paired device id with its own key.
        let (mut engine, desktop) = engine();
        let paired = Phone::new();
        let config = config_with(&paired);
        let mut impostor = Phone::new();
        let mut pairing = Pairing::default();
        let ephemeral = KeyPair::generate().unwrap();

        let hello = impostor.route(&WireFrame::Hello {
            ephemeral: *ephemeral.point(),
        });
        let actions = engine.on_route(hello, &config, &mut pairing, Instant::now());
        // The impostor derives different keys: CONFIRM does not open.
        let WireFrame::HelloAck {
            ephemeral: desktop_ephemeral,
            confirm,
        } = decode_sent(&actions[0])
        else {
            panic!("expected ack");
        };
        let handshake = Handshake {
            mode: Mode::Session,
            sid: SID,
            desktop_static: *desktop.point(),
            mobile_static: *impostor.static_key.point(),
            desktop_ephemeral,
            mobile_ephemeral: *ephemeral.point(),
            psk: None,
        };
        let mut channel =
            crypto::derive(Role::Mobile, &impostor.static_key, &ephemeral, &handshake).unwrap();
        assert!(channel.open(0, &confirm).is_err());

        // Whatever it then sends fails to decrypt and kills the session.
        let (seq, ciphertext) = channel.seal(br#"{"id":1,"op":"roots.list"}"#).unwrap();
        let forged = RouteIn {
            from: PHONE,
            sid: SID.to_string(),
            seq,
            payload: wire::encode(&WireFrame::Data { ciphertext }),
        };
        let actions = engine.on_route(forged, &config, &mut pairing, Instant::now());
        assert_eq!(actions, vec![Action::SessionClosed { device_id: PHONE }]);
    }

    #[test]
    fn replayed_request_closes_the_session() {
        let (mut engine, desktop) = engine();
        let mut phone = Phone::new();
        let config = config_with(&phone);
        let mut pairing = Pairing::default();
        let ephemeral = KeyPair::generate().unwrap();
        let hello = phone.route(&WireFrame::Hello {
            ephemeral: *ephemeral.point(),
        });
        let actions = engine.on_route(hello, &config, &mut pairing, Instant::now());
        phone.finish(Mode::Session, &ephemeral, &desktop, &actions[0], None);

        let request = phone.data(r#"{"id":1,"op":"roots.list"}"#);
        engine.on_route(request.clone(), &config, &mut pairing, Instant::now());
        let actions = engine.on_route(request, &config, &mut pairing, Instant::now());
        assert_eq!(actions, vec![Action::SessionClosed { device_id: PHONE }]);
        assert!(engine.active_devices().is_empty());
    }

    #[test]
    fn pairing_flow_needs_code_proof_and_user_confirmation() {
        let (mut engine, desktop) = engine();
        let mut phone = Phone::new();
        let config = RemoteConfig::default();
        let mut pairing = Pairing::default();
        let now = Instant::now();
        let qr = pairing.start("1", &desktop.spki(), now).unwrap();
        assert_eq!(qr.k, BASE64.encode(desktop.spki()));
        let ephemeral = KeyPair::generate().unwrap();

        let hello = phone.route(&WireFrame::PairHello {
            ephemeral: *ephemeral.point(),
            static_spki: phone.static_key.spki(),
        });
        let actions = engine.on_route(hello, &config, &mut pairing, now);
        let [Action::VerifyPairing {
            from,
            sid,
            ephemeral: mobile_ephemeral,
            static_spki,
            code,
        }] = actions.as_slice()
        else {
            panic!("expected verify, got {actions:?}");
        };
        let code = *code;
        // Registry check passed (done by the service loop).
        let actions = engine.accept_pairing(
            *from,
            &sid.clone(),
            *mobile_ephemeral,
            &static_spki.clone(),
            &code,
            "Pixel".to_string(),
        );
        phone.finish(Mode::Pairing, &ephemeral, &desktop, &actions[0], Some(&code[..]));
        // Not serving yet: a pairing session never answers requests.
        assert!(engine.active_devices().is_empty());

        let proof = phone.data(r#"{"op":"pair.hello"}"#);
        let actions = engine.on_route(proof, &config, &mut pairing, now);
        let [Action::PairingProved { device_id, sas, public_key, .. }] = actions.as_slice() else {
            panic!("expected proof, got {actions:?}");
        };
        assert_eq!(*device_id, PHONE);
        assert_eq!(*sas, phone.channel.as_ref().unwrap().sas());
        assert_eq!(*public_key, BASE64.encode(phone.static_key.spki()));

        let actions = engine.finish_pairing(PHONE, true);
        assert_eq!(phone.open(&actions[0])["t"], "pair.ok");
        assert_eq!(actions[1], Action::SessionClosed { device_id: PHONE });

        // The code was single use.
        let again = phone.route(&WireFrame::PairHello {
            ephemeral: *ephemeral.point(),
            static_spki: phone.static_key.spki(),
        });
        let actions = engine.on_route(again, &config, &mut pairing, now);
        assert_eq!(
            decode_sent(&actions[0]),
            WireFrame::Reject {
                code: RejectCode::PairingClosed as u8
            }
        );
    }

    #[test]
    fn rate_limits_in_flight_requests() {
        let (mut engine, desktop) = engine();
        let mut phone = Phone::new();
        let config = config_with(&phone);
        let mut pairing = Pairing::default();
        let ephemeral = KeyPair::generate().unwrap();
        let hello = phone.route(&WireFrame::Hello {
            ephemeral: *ephemeral.point(),
        });
        let actions = engine.on_route(hello, &config, &mut pairing, Instant::now());
        phone.finish(Mode::Session, &ephemeral, &desktop, &actions[0], None);

        for id in 0..MAX_IN_FLIGHT {
            let request = phone.data(&format!(r#"{{"id":{id},"op":"roots.list"}}"#));
            let actions = engine.on_route(request, &config, &mut pairing, Instant::now());
            assert!(matches!(actions[0], Action::Serve { .. }));
        }
        let request = phone.data(r#"{"id":99,"op":"roots.list"}"#);
        let actions = engine.on_route(request, &config, &mut pairing, Instant::now());
        assert_eq!(phone.open(&actions[0])["error"], "RATE_LIMITED");
    }
}

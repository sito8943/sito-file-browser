# Remote Control Plan (0.18.0)

Plan creado el 1 de noviembre de 2026.

## Estado

| Fase | Estado |
| --- | --- |
| 1. sito-auth `/devices` | Hecha y desplegada (PR https://github.com/sito8943/sito-auth-server/pull/2). Contrato en su `readme.md`. |
| 2. Relay | Hecha y desplegada en `wss://api.remote.sito8943.com/ws` (repo privado https://github.com/sito8943/sito-remote-relay). Contrato del wire en su `readme.md`. |
| 3. Desktop | Backend Rust hecho en la rama `0.18.0` (sin commitear). Probado el 2 de noviembre de 2026: login en sito-auth, registro del equipo y conexión `online` al relay. Frontend hecho (`src/features/remote` + "Phone access" en Ajustes); probado: lint/tsc, compartir carpetas y QR de emparejamiento. El emparejamiento completo espera a la app Android. |
| 4. Android | Pendiente |
| 5. Endurecimiento | Pendiente |

Diferencias con el diseño original, ya implementadas en el relay: la sesión WebSocket vive hasta el
`exp` del access token y el cliente la extiende con `{"t":"auth","token":...}` (cierre `4401` si no);
esto cubre revocación de dispositivo y "sign out everywhere" sin polling. El id de dispositivo de
sito-auth es numérico (`Long`), no UUID; el desktop lo maneja como string. sito-auth rota el refresh
token en cada `/auth/refresh`, así que los refresh van serializados. Un dispositivo solo tiene
un socket vivo (el nuevo cierra el viejo con `4000`). Se rechazan handshakes con cabecera `Origin`.

Producción (1 de noviembre de 2026): sito-auth con `/devices` (migración V2 aplicada) y relay en el
VPS (`~/servers/sito-remote-relay`, puerto host 8084, `mem_limit` 384m, certificado y
`infra/nginx/conf.d/remote.conf` activos). Verificado: health `UP`, handshake sin token `401`. Aún
no se probó una sesión real con token + dispositivo (no hay cliente hasta la Fase 3).

### Para retomar (Fase 3)

1. Decidir la rama de trabajo en este repo (propuesta: `0.18.0` desde `main`). Este plan y la línea
   en `PROJECT_CONTEXT.md` siguen sin commitear y van en esa rama.
2. Empezar por Rust: `src-tauri/src/remote/` (auth contra sito-auth, cliente WSS, crypto, pairing,
   handlers de solo lectura sobre los cores de `fs.rs`), luego el setting `remoteEnabled` y la
   feature `src/features/remote`.
3. Habilitar `keyring` en Windows y Linux (hoy solo macOS en `Cargo.toml`).
4. Primera prueba real de punta a punta contra `wss://api.remote.sito8943.com/ws`.

Pendientes de infraestructura (fuera de este repo):

- `~/servers/infra` en el VPS tiene sin commitear `conf.d/remote.conf` y `chat.conf.disabled`,
  junto con cambios de otra sesión (`renderdiff.conf` borrado, `conf.d.real/`).
- Mientras chat está apagado su vhost es `chat.conf.disabled`; al reactivarlo, renombrar y recargar
  nginx después de levantar el contenedor.
- Propuesta: `resolver 127.0.0.11` + `proxy_pass` con variable en nginx, para que recrear un
  contenedor no deje 502 por IP vieja ni un upstream apagado bloquee `nginx -s reload`.

## Objetivo

Desde una app Android (Kotlin) el usuario navega carpetas autorizadas de su portátil con Sito File
Browser abierto, y copia archivos al móvil, **desde cualquier red**. Portátil y móvil se conectan
hacia afuera a un relay en el VPS; la cuenta es la de sito-auth.

```text
Sito (Tauri/Rust) ──WSS saliente──┐
                                  ├── sito-remote-relay (VPS) ── valida JWT vía JWKS de sito-auth
App Android (Kotlin) ──WSS saliente┘        enruta por user + device, nunca ve texto plano
```

## Reglas de seguridad (fijas, no negociables)

1. **Solo lectura + copiar al móvil.** No existen comandos de escritura (borrar, mover, renombrar,
   crear, abrir en el PC). El desktop rechaza cualquier tipo de request fuera de la allowlist.
2. **Acceso solo a carpetas autorizadas**, nunca al disco entero. Cada ruta se canonicaliza
   (resuelve `..` y symlinks) y debe quedar dentro de una raíz autorizada; si no, `FORBIDDEN`.
3. **Remote apagado por defecto.** Con el toggle apagado no se abre ninguna conexión al relay.
4. **Emparejamiento explícito por dispositivo**, con pasos de seguridad: QR de un uso + código de
   verificación (SAS) confirmado en el portátil. Nada de plug-and-play. Revocable desde el portátil.
5. **Cifrado E2E.** El relay y el VPS solo ven sobres cifrados y metadatos de enrutamiento.

La frontera de seguridad real es el **desktop**: aunque roben la cuenta de sito-auth o comprometan
el relay, el desktop solo acepta sesiones de claves públicas que él mismo emparejó.

## Piezas y repositorios

| Pieza | Repo | Estado |
| --- | --- | --- |
| Registro de dispositivos | `servers/auth` (sito-auth, Spring Boot 3.5, Java 21, MySQL, Flyway) | Extender |
| Relay | `servers/sito-remote-relay` (nuevo, Spring Boot 3.5, Java 21) | Nuevo |
| Proxy | `servers/infra/nginx/conf.d/remote.conf` | Nuevo |
| Desktop | este repo: `src-tauri/src/remote/` + `src/features/remote` | Nuevo módulo |
| App Android | `mobile/SitoFileRemote` (nuevo, mismas reglas que `mobile/SitoChat`) | Nuevo |

Por qué un relay nuevo y no extender `sito-chat-server`: chat es STOMP + MySQL + persistencia de
mensajes y sus reglas prohíben ampliar el alcance; el relay es transporte efímero sin DB.

## Fase 1 — sito-auth: dispositivos

Hoy sito-auth solo tiene `user` y `refresh_token`; no hay concepto de dispositivo.

- Migración `V2__devices.sql`: tabla `device` (`id` UUID, `user_id` FK, `name`, `platform`
  `macos|windows|linux|android`, `kind` `desktop|mobile`, `public_key` P-256 SPKI base64,
  `created_at`, `last_seen_at`, `revoked_at`).
- Endpoints autenticados (`/devices/**`, cubiertos por `anyRequest().authenticated()`):
  - `POST /devices` registra el dispositivo actual (nombre, plataforma, tipo, clave pública).
  - `GET /devices` lista los del usuario (sin revocados).
  - `DELETE /devices/{id}` revoca.
  - `GET /devices/{id}` la usa el relay al conectar para confirmar dueño y no revocado.
- Capas según `../wallet/AGENTS.md`: controller → `services/device` → `application/device` →
  `domain`. Contrato documentado en `readme.md`. `./mvnw test` lo corre el developer.

El registro de sito-auth sirve para **listar y enrutar**; la confianza criptográfica vive en el
desktop (regla de frontera arriba).

## Fase 2 — sito-remote-relay

- Spring Boot 3.5 / Java 21, **WebSocket crudo** (no STOMP: el cliente Rust no necesita un broker y
  así no hay que escribir un cliente STOMP en Rust).
- Auth: `Authorization: Bearer <jwt>` en el handshake. Validación vía JWKS
  (`http://sito-auth:8081/.well-known/jwks.json`, red `wallet_default`), `iss=sito-auth`,
  `type=access`, `sub` = userId. Además `X-Device-Id`, verificado contra `GET /devices/{id}`
  (caché corta). Copiar el patrón de `wallet` (`SitoAuthResourceServerTest`, `StompAuthenticationTest`).
- Presencia en memoria: `userId → {deviceId → session}`. Un dispositivo solo ve los dispositivos
  online de su mismo usuario. Una sola instancia, sin Redis (igual que chat).
- Sobre visible para el relay (JSON):
  `{ "t": "route", "to": "<deviceId>", "from": "<deviceId>", "sid": "<sessionId>", "seq": n, "p": "<base64 ciphertext>" }`
  más `hello`, `presence`, `ping`. El relay nunca interpreta `p`.
- Límites: tamaño máximo de mensaje (p. ej. 512 KB), tasa por dispositivo, cuota diaria de bytes
  por usuario. Nunca loguear `p`, tokens ni claves.
- Revocación: el relay vuelve a validar el dispositivo cada N minutos y cierra la sesión si fue
  revocado. El access token dura 15 min; el cliente reconecta con un token renovado.
- Deploy: `docker-compose.yml` en `proxy-net` + `wallet_default`; nginx `remote.conf` con
  subdominio propio (p. ej. `api.remote.sito8943.com`), certbot, bloque WS (`Upgrade`,
  `Connection "upgrade"`) y `proxy_read_timeout` mayor que el heartbeat.

## Criptografía E2E

Primitivas iguales a SitoChat para reutilizar código Android (JCA, sin librerías externas):
**P-256 ECDH + HKDF-SHA256 + AES-256-GCM**. En Rust: `aws-lc-rs` (ya compilado por russh/rustls;
no añade crates). Especificación exacta en "Protocolo E2E v1" más abajo.

- Cada dispositivo tiene una clave estática P-256. Desktop: privada en el llavero del SO
  (`keyring`). Android: envuelta por una clave AES-GCM del Android Keystore (patrón de
  `AndroidMessageCrypto`).
- Sesión: cada lado genera una clave efímera. Se combinan los cuatro ECDH estilo Noise `KK`
  (efímera·efímera, efímera·estática en ambos sentidos, estática·estática). Da autenticación mutua,
  forward secrecy y evita que una sola clave estática filtrada permita suplantar al otro lado.
- Nonce: contador por dirección + `sid`; se rechazan duplicados y saltos (anti-replay).
- **Revisión obligatoria** del handshake por alguien con criterio criptográfico antes del release.
  Si se prefiere no diseñar handshake propio, alternativa: Noise `KK` (`snow` en Rust; en Kotlin
  habría que portar o usar `noise-java`).

## Emparejamiento

1. Portátil: **Ajustes → Remote → Emparejar dispositivo**. Muestra un QR con `desktopDeviceId`,
   clave pública del desktop, código de un uso de 128 bits (expira en 2 min) y URL del relay.
2. Móvil (con sesión de sito-auth iniciada) escanea el QR, se registra en sito-auth si hace falta y
   envía por el relay `pair.request` cifrado hacia la clave del desktop, con su clave pública y el
   código.
3. El desktop valida el código (un uso, no expirado, mismo usuario). El móvil muestra un **SAS de
   6 dígitos** = hash(clave desktop ‖ clave móvil ‖ código); el desktop no lo muestra.
4. El usuario escribe ese SAS en el portátil (un intento; el backend compara). Solo si coincide el
   desktop guarda la clave del móvil en su lista de confianza.

Revocar en el desktop borra la clave de su lista (efecto inmediato) y llama `DELETE /devices/{id}`.

## Protocolo E2E v1 (lo que la app Android implementa byte a byte)

Implementación de referencia: `src-tauri/src/remote/crypto.rs` y `wire.rs`, con tests.

Notación: `‖` concatena; puntos P-256 sin comprimir (65 bytes, `0x04 ‖ X ‖ Y`); claves estáticas
en el registro y en el QR como SPKI DER base64 (91 bytes = prefijo fijo de 26 bytes + punto);
ECDH devuelve la coordenada X (32 bytes, igual que `KeyAgreement("ECDH")` de JCA).

**Sobre del relay.** `p` = base64 de un frame binario:

| Frame | Bytes | Sentido | `seq` |
| --- | --- | --- | --- |
| `HELLO` | `0x01 ‖ 0x01(versión) ‖ eM(65)` | m→d | 0 |
| `PAIR_HELLO` | `0x02 ‖ 0x01 ‖ eM(65) ‖ SPKI sM(91)` | m→d | 0 |
| `HELLO_ACK` | `0x11 ‖ eD(65) ‖ seal(CONFIRM)` | d→m | 0 |
| `PAIR_ACK` | `0x12 ‖ eD(65) ‖ seal(CONFIRM)` | d→m | 0 |
| `DATA` | `0x03 ‖ ciphertext‖tag` | ambos | m→d desde 1, d→m desde 1 |
| `REJECT` | `0x7F ‖ código` (sin autenticar) | d→m | — |

Códigos de `REJECT`: 1 no emparejado, 2 emparejamiento cerrado, 3 el registro no coincide,
4 versión no soportada, 5 ocupado.

**Derivación.** `sid` lo elige el móvil (≤ 64 bytes ASCII, aleatorio, p. ej. 32 hex).

```text
LABEL = "sito-remote/v1"
mode  = 0x01 sesión | 0x02 emparejamiento
salt  = SHA-256(LABEL ‖ mode ‖ u8(len sid) ‖ sid ‖ sD ‖ sM ‖ eD ‖ eM)
IKM   = DH(eD,eM) ‖ DH(eD,sM) ‖ DH(sD,eM) ‖ DH(sD,sM) [‖ código QR (16 bytes), solo emparejamiento]
PRK   = HKDF-Extract(salt, IKM)
k_m2d = HKDF-Expand(PRK, "sito-remote/v1 m2d", 32)
k_d2m = HKDF-Expand(PRK, "sito-remote/v1 d2m", 32)
SAS   = u32_be(HKDF-Expand(PRK, "sito-remote/v1 sas", 4)) mod 1_000_000, en 6 dígitos
```

**Mensajes.** AES-256-GCM, tag 16 bytes. `nonce = 0x00000000 ‖ u64_be(seq)`.
`AAD = LABEL ‖ dir ‖ u8(len sid) ‖ sid ‖ u64_be(seq)` con `dir` = `0x01` d→m, `0x02` m→d. El `seq`
es el del sobre del relay; el receptor exige exactamente el siguiente (ni repetidos ni saltos) y
cualquier fallo de descifrado termina la sesión. `CONFIRM = "sito-remote/v1 confirm"`, sellado por
el desktop con `seq` 0; el móvil lo verifica antes de enviar nada.

**Sesión.** Móvil: `HELLO` → desktop busca `from` en su lista de confianza (si no, `REJECT 1`),
deriva y responde `HELLO_ACK` → el móvil verifica `CONFIRM` y envía requests en `DATA`. Un `HELLO`
nuevo del mismo dispositivo reemplaza la sesión anterior.

**Emparejamiento.** Móvil: `PAIR_HELLO` → el desktop consume el código (un uso, aunque falle),
comprueba en sito-auth `GET /devices/{from}` que existe, es `MOBILE` y su `publicKey` es la `sM`
recibida (si no, `REJECT 3`), deriva con el código y responde `PAIR_ACK`. El primer `DATA` del
móvil (`{"op":"pair.hello"}`) prueba que conoce el código; solo entonces el móvil muestra el SAS
y el portátil pide escribirlo (el SAS nunca llega a la UI del portátil; el nombre del móvil sale
del registro de sito-auth, no de lo que diga el móvil). Si lo escrito coincide (comparación en
tiempo constante, un intento), el desktop guarda la clave y responde `{"t":"pair.ok"}`; si no
coincide o el usuario rechaza, `{"t":"pair.rejected"}` y cierra.

## Protocolo dentro del canal cifrado

Allowlist cerrada, todo de solo lectura:

| Request | Respuesta | Core reutilizado |
| --- | --- | --- |
| `roots.list` | raíces autorizadas (nombre visible, id) | `remote.toml` |
| `dir.list {root, path}` | entradas | `read_directory_local` |
| `entry.stat {root, path}` | metadata | `get_entry_local` |
| `search {root, path, query}` | entradas | `search_directory_core` |
| `thumb.get {root, path, size}` | imagen | pipeline de `get_thumbnail` (SVG con resvg, sin referencias externas) |
| `file.read {root, path, offset, len}` | chunk (256 KB) | lectura en bloques nueva, en worker |

- Las rutas viajan **relativas a una raíz** (`root` id + `path`), nunca absolutas: el móvil no
  conoce ni puede pedir rutas fuera de las raíces.
- Validación en el desktop: unir raíz + ruta, canonicalizar, comprobar prefijo de la raíz
  canonicalizada; rechazar symlinks que salgan; solo rutas locales (sin `sftp://`, `smb://`,
  Recents ni tags virtuales).
- Descarga: en bloques, reanudable por `offset`, límite de tamaño configurable (propuesta 500 MB).
- Errores tipados: `FORBIDDEN`, `NOT_FOUND`, `TOO_LARGE`, `RATE_LIMITED`, `INTERNAL`.

## Fase 3 — Desktop (este repo)

Rust, módulo nuevo `src-tauri/src/remote/`:

- `mod.rs`: ciclo de vida del servicio (arranca/para con el setting), tarea tokio, nunca en el hilo
  principal; el trabajo de disco va a `spawn_blocking`.
- `relay_client.rs`: WSS (`tokio-tungstenite` + rustls), heartbeat, reconexión con backoff.
- `auth.rs`: login a sito-auth, refresh token en el llavero, renovación del access token.
- `crypto.rs`, `pairing.rs`: handshake, SAS, lista de confianza.
- `handlers.rs`: allowlist de requests → cores de `filesystem/fs.rs`, validación de raíces.
- `store.rs`: `remote.toml` (raíces autorizadas y dispositivos emparejados: id, nombre, clave
  pública; nada secreto), siguiendo el patrón de `cleanup.toml`.
- Comandos Tauri como adaptadores, registrados en `main.rs`; wrappers en `api.ts`.
- `tokio` necesita las features `net` y `time`. La red sale desde Rust, así que el CSP del webview
  no cambia.

Frontend, feature nueva `src/features/remote` (expuesta por su `index.ts`):

- Setting `remoteEnabled` (default `false`) en los siete puntos del contrato de Settings.
- Sección **Remote** en `SETTINGS_SECTIONS` con controles `SETTING_KIND.CUSTOM`: cuenta (login y
  logout), carpetas autorizadas (añadir/quitar, como el cleanup watch list), dispositivos
  emparejados (revocar), botón de emparejar.
- `PairDeviceDialog` (Dialog estándar) con QR y campo para escribir el SAS que muestra el móvil.
- Indicador visible cuando hay una sesión remota activa, y toast al conectarse un dispositivo.
- `RemoteManager` + `RemoteProvider`; textos en `src/lang/en.ts`; CSS con tokens.

## Fase 4 — App Android `mobile/SitoFileRemote`

Mismas reglas que SitoChat: un módulo, Compose + Material 3, OkHttp (REST y WebSocket), sin
framework de DI, `MainActivity` como composition root, ViewModels con `StateFlow`.

- Reutilizar patrones: `AndroidKeystoreSessionStore` (sesión), `AndroidMessageCrypto` (P-256),
  gateway WebSocket con heartbeat y reconexión acotada (de `OkHttpChatGateway`, sin STOMP).
- Pantallas: Login → Dispositivos (online/offline) → Emparejar (cámara + QR + SAS) → Explorador
  (raíces → carpetas, miniaturas, búsqueda) → Descargas (progreso, reanudar, destino
  `MediaStore.Downloads`).
- UI del explorador sobre una interfaz `FileSource` con una sola implementación
  `RemoteFileSource`; explorar archivos locales del móvil queda fuera de 0.18.0.

## Fase 5 — Endurecimiento y release

- Revisión del handshake y de la validación de rutas (tests de escape: `..`, symlinks, mayúsculas
  en macOS, rutas Unicode).
- Pruebas en red real: portátil y móvil en redes distintas, portátil dormido, relay reiniciado,
  token expirado a mitad de una descarga, dispositivo revocado a mitad de sesión.
- Documentar en `README.md` y `CHANGELOG.md`; actualizar `PROJECT_CONTEXT.md` con el módulo
  `remote`.

## Fuera de alcance de 0.18.0

Escritura de cualquier tipo, abrir archivos en el PC, controlar la ventana de Sito, P2P/WebRTC,
conexión directa en LAN, explorar archivos locales del móvil, iOS, acceso a SFTP/SMB por remoto.

## Decisiones (cerradas el 1 de noviembre de 2026)

1. **Plataformas del desktop:** habilitar `keyring` en Windows (`windows-native`) y Linux
   (Secret Service) además de macOS; remote en las tres plataformas.
2. **Subdominio** del relay: `api.remote.sito8943.com`.
3. **Límite de descarga:** 500 MB por archivo + cuota diaria por usuario en el relay.
4. **Handshake propio** P-256 alineado con SitoChat, con revisión criptográfica antes del release.
5. **Application ID** de la app Android: `com.sito8943.fileremote`, en su propio repo.

Cerradas el 2 de noviembre de 2026:

6. **Archivos ocultos:** setting `remoteShowHidden` (default `false`). Apagado, los dotfiles no
   aparecen en listados ni búsquedas y pedir uno responde `NOT_FOUND`.
7. **Sistemas de archivos de red bloqueados:** una raíz en SMB/NFS/AFP/WebDAV o unidad de red
   mapeada se rechaza (`REMOTE_ROOT_NETWORK`), y una ruta que cruce a un montaje de red dentro de
   una raíz responde `FORBIDDEN`. En Linux, sshfs (FUSE) no se detecta.

## Impacto en otros consumidores de sito-auth (wallet, notas)

Ninguno, siempre que la Fase 1 sea **solo aditiva**:

- No cambiar claims del JWT, la clave de firma, el JWKS ni los endpoints `/auth/*` (wallet y
  sito-notas solo usan `sign-in`, `sign-out`, `refresh`, `session` y `password/*`).
- No tocar la tabla `user`; `device` solo añade una FK hacia ella.
- Único efecto operativo: el redeploy de sito-auth corta logins/refresh unos segundos. Los access
  tokens ya emitidos siguen validando en wallet porque la verificación es local vía JWKS.

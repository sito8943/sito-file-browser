# Revisión criptográfica: control remoto de Sito File Browser

Documento para pedir una revisión a alguien con formación en criptografía (por ejemplo, un
profesor). Explica qué protege el sistema, cómo funciona y qué preguntas concretas queremos que
responda. No hace falta leer código; al final están los archivos por si quiere verlo.

Versión del protocolo: `sito-remote/v1`. Fecha: 2 de noviembre de 2026.

## 1. Qué es y qué queremos proteger

Sito File Browser es un explorador de archivos de escritorio (macOS, Windows, Linux). La versión
0.18.0 añade una app Android que permite **ver carpetas del portátil y copiar archivos al móvil
desde cualquier red**.

Como portátil y móvil suelen estar detrás de NAT, ninguno acepta conexiones entrantes. Ambos se
conectan hacia afuera a un **servidor relay** propio, que reenvía mensajes entre ellos.

```text
Portátil ──WebSocket TLS──► Relay (VPS) ◄──WebSocket TLS── Móvil
                              │
                  reenvía mensajes por id de dispositivo
```

**Objetivo de seguridad principal:** que ni el relay, ni quien controle el VPS, ni quien robe la
cuenta del usuario, pueda leer los archivos o pedirle archivos al portátil.

Reglas fijas del producto (limitan el daño si algo falla):

1. Solo lectura: listar, buscar, ver miniaturas y descargar. No existe ningún comando para borrar,
   mover ni escribir.
2. Solo carpetas que el usuario autorizó explícitamente, nunca el disco entero.
3. Apagado por defecto.
4. Cada móvil se empareja a mano, con un código QR y un código de 6 dígitos que el usuario
   escribe en el portátil.
5. Cifrado de extremo a extremo: el relay solo ve datos cifrados.

## 2. Modelo de amenazas

| Atacante | Qué puede hacer | Qué debe seguir siendo imposible |
| --- | --- | --- |
| El relay o el VPS comprometido | Leer, borrar, reordenar, repetir o inyectar mensajes | Leer contenido, suplantar a un dispositivo, emparejar un móvil propio |
| Robo de la cuenta del usuario (sito-auth) | Registrar dispositivos nuevos en la cuenta | Acceder al portátil sin pasar por el emparejamiento físico |
| Alguien que ve el QR (por encima del hombro) | Intentar emparejarse en esos 2 minutos | Emparejarse sin que el usuario lo note (lo frena el código de 6 dígitos que hay que escribir) |
| Robo posterior de una clave estática | Descifrar tráfico grabado | Descifrar sesiones pasadas (forward secrecy) |

Fuera de alcance: un portátil o un móvil ya comprometidos (malware en el propio dispositivo) y la
denegación de servicio (el relay siempre puede dejar de reenviar).

La **frontera de confianza real es el portátil**: solo atiende a móviles cuya clave pública él
mismo guardó durante el emparejamiento. El registro de dispositivos del servidor de cuentas sirve
para enrutar mensajes, no para decidir en quién confiar.

## 3. Primitivas

Todo sobre la curva **P-256** (secp256r1), por compatibilidad con Android sin librerías externas:

- **ECDH P-256**: el secreto compartido es la coordenada X (32 bytes).
- **HKDF-SHA256** (RFC 5869) para derivar claves.
- **AES-256-GCM** con tag de 16 bytes para cifrar mensajes.
- **SHA-256** para resumir el transcript.

Implementación en el portátil: `aws-lc-rs` (librería de AWS, derivada de BoringSSL). En Android:
JCA estándar (`KeyAgreement("ECDH")`, `Cipher("AES/GCM/NoPadding")`, HMAC para HKDF).

## 4. Claves

Cada dispositivo tiene:

- **Una clave estática** P-256, de larga duración. Identifica al dispositivo.
  - Portátil: la privada se guarda en el llavero del sistema operativo (Keychain, Credential
    Manager, Secret Service).
  - Móvil: la privada se guarda cifrada con una clave del Android Keystore.
- **Una clave efímera** P-256 nueva en cada sesión, que se descarta al terminar.

Notación: `sD`, `sM` = claves estáticas del portátil (D, desktop) y del móvil (M). `eD`, `eM` =
efímeras. `DH(a, b)` = ECDH entre la privada de un lado y la pública del otro.

## 5. Cómo se establece una sesión (dispositivo ya emparejado)

Se inspira en el patrón **KK** del framework Noise (ambos lados ya conocen la clave estática del
otro), pero no es una implementación de Noise.

```text
Móvil                                              Portátil
  │  HELLO: eM (pública efímera)                      │
  │ ────────────────────────────────────────────────► │  ¿el remitente está emparejado?
  │                                                   │  calcula las claves
  │       HELLO_ACK: eD + cifrado("confirm")          │
  │ ◄──────────────────────────────────────────────── │
  │  calcula las claves, comprueba "confirm"          │
  │  DATA: peticiones cifradas                        │
  │ ◄───────────────────────────────────────────────► │
```

**Derivación de claves:**

```text
IKM  = DH(eD,eM) ‖ DH(eD,sM) ‖ DH(sD,eM) ‖ DH(sD,sM)
salt = SHA-256( "sito-remote/v1" ‖ modo ‖ largo(sid) ‖ sid ‖ sD ‖ sM ‖ eD ‖ eM )
PRK  = HKDF-Extract(salt, IKM)

clave móvil→portátil = HKDF-Expand(PRK, "sito-remote/v1 m2d", 32 bytes)
clave portátil→móvil = HKDF-Expand(PRK, "sito-remote/v1 d2m", 32 bytes)
código SAS           = HKDF-Expand(PRK, "sito-remote/v1 sas", 4 bytes) mod 1 000 000
```

- `sid` es un identificador de sesión aleatorio elegido por el móvil.
- `modo` vale 1 para sesión y 2 para emparejamiento, para que un mensaje de un tipo no se pueda
  reutilizar como del otro.

**Por qué cuatro ECDH y no uno:**

| Término | Aporta |
| --- | --- |
| `DH(eD,eM)` | Forward secrecy: sin las efímeras, el tráfico grabado no se descifra |
| `DH(eD,sM)` | Solo el dueño real de `sM` puede calcularlo: autentica al móvil |
| `DH(sD,eM)` | Solo el dueño real de `sD` puede calcularlo: autentica al portátil |
| `DH(sD,sM)` | Une la sesión a la pareja concreta de dispositivos emparejados |

La autenticación es **implícita**: quien no tenga la clave privada correcta obtiene claves
distintas y su primer mensaje no se descifra. El portátil demuestra explícitamente que derivó lo
mismo cifrando un texto fijo (`"sito-remote/v1 confirm"`) en su respuesta.

## 6. Cifrado de los mensajes

- AES-256-GCM, **una clave distinta por sentido**.
- `nonce` (12 bytes) = 4 bytes a cero ‖ número de secuencia de 64 bits.
- Datos autenticados (AAD) = `"sito-remote/v1" ‖ sentido ‖ sid ‖ número de secuencia`.
- Cada sentido numera sus mensajes 0, 1, 2… El receptor exige **exactamente el siguiente**; un
  mensaje repetido, saltado o reordenado se rechaza y la sesión se cierra.
- Cada sesión empieza con claves nuevas, así que un número de secuencia nunca se reutiliza con la
  misma clave.

## 7. Emparejamiento (primera vez)

```text
1. Portátil muestra un QR: id del portátil, su clave pública sD, un código aleatorio de
   128 bits (válido 2 minutos, un solo uso) y la URL del relay.
2. El móvil lo escanea y envía PAIR_HELLO: eM + su clave pública sM.
3. El portátil:
   - consume el código (queda inválido aunque el intento falle);
   - comprueba en el servidor de cuentas que ese dispositivo existe, es un móvil y tiene la
     clave sM recibida;
   - deriva las claves como en la sección 5, añadiendo el código al final del IKM (como clave
     precompartida) y con modo = 2;
   - responde PAIR_ACK con eD + "confirm" cifrado.
4. El móvil envía su primer mensaje cifrado. Si se descifra, el portátil sabe que el móvil
   conoce el código y posee la privada de sM.
5. El móvil muestra un código de 6 dígitos (SAS). El portátil **no** lo muestra: el usuario
   tiene que escribirlo en el portátil, y el portátil lo compara (en tiempo constante) con el
   suyo. Un solo intento: si no coincide, rechaza el móvil y hay que empezar de nuevo.
6. Solo si coincide, el portátil guarda sM en su lista de confianza.
```

El código del QR **nunca se compara**: si es incorrecto, las claves salen distintas y el
descifrado falla. Revocar un móvil en el portátil borra su clave de la lista con efecto inmediato.

## 8. Formato en el cable

El relay ve un sobre JSON con origen, destino, `sid`, número de secuencia y `p` (los datos en
base64). Dentro de `p` va un frame binario:

| Frame | Contenido |
| --- | --- |
| `HELLO` | tipo ‖ versión ‖ eM |
| `PAIR_HELLO` | tipo ‖ versión ‖ eM ‖ sM |
| `HELLO_ACK` / `PAIR_ACK` | tipo ‖ eD ‖ "confirm" cifrado |
| `DATA` | tipo ‖ texto cifrado ‖ tag |
| `REJECT` | tipo ‖ código de error (sin cifrar ni autenticar; el móvil solo lo usa para mostrar un aviso) |

Las claves públicas viajan como puntos sin comprimir (65 bytes). El portátil rechaza puntos que no
estén en la curva.

## 9. Debilidades conocidas y decisiones

1. **Protocolo propio.** No es Noise ni TLS. Por eso pedimos esta revisión. Alternativa sin diseño
   propio: usar Noise `KK` con una librería auditada (`snow` en Rust); en Android habría que usar
   `noise-java` o portarla.
2. **No hay ratchet** dentro de una sesión. Si se filtra la clave de una sesión, se lee esa sesión
   entera (pero no las demás).
3. **El primer mensaje del móvil no está autenticado** (`HELLO` solo trae eM). Un atacante puede
   enviar HELLO falsos; el portátil gasta 4 ECDH por cada uno, pero nunca llega a descifrar nada.
4. **El SAS tiene 6 dígitos** (unos 20 bits). Para engañarlo, un atacante tendría que estar activo
   en ese mismo minuto y haber visto el QR, y acertar con probabilidad 1 entre 1 000 000 en su
   único intento. Se escribe en vez de compararse porque comparar invita a pulsar "coinciden"
   sin mirar.
5. **Metadatos visibles para el relay:** qué dispositivos hablan, cuándo, y el tamaño de cada
   mensaje.

## 10. Preguntas para el revisor

1. ¿La combinación de los cuatro ECDH da autenticación mutua y forward secrecy, como esperamos?
2. ¿Hay algún ataque de suplantación con una clave filtrada (KCI) que se nos escape?
3. ¿Es correcto usar el transcript resumido como `salt` de HKDF, o debería ir en el campo `info`?
4. ¿Mezclar el código del QR al final del IKM es una forma segura de usarlo como clave
   precompartida? ¿Bastan 128 bits y un solo uso?
5. ¿El SAS de 6 dígitos derivado del PRK, escrito por el usuario en el portátil con un solo
   intento, es suficiente para detectar un intermediario durante el emparejamiento?
6. ¿El nonce (contador) + AAD con sentido y `sid` evita reutilizar nonces y reflejar mensajes?
7. ¿Falta algo en el transcript (versión, ids de dispositivo, roles)?
8. ¿Recomendaría pasar directamente a Noise `KK` en lugar de este diseño?
9. Cualquier otra cosa que no haríamos así.

## 11. Para quien quiera ver el código

- Implementación de referencia: `src-tauri/src/remote/crypto.rs` (derivación y cifrado) y
  `src-tauri/src/remote/wire.rs` (formato de los frames). Incluyen tests que simulan ambos lados.
- Emparejamiento: `src-tauri/src/remote/pairing.rs`.
- Especificación técnica compacta: `plans/REMOTE_CONTROL_PLAN.md`, sección "Protocolo E2E v1".

// Service lifecycle states. Mirrors RemoteState in src-tauri/src/remote/service.rs.
export const REMOTE_STATE = {
  OFF: "off",
  STARTING: "starting",
  CONNECTING: "connecting",
  ONLINE: "online",
  STOPPED: "stopped",
} as const;

export const PAIR_DEVICE_TITLE_ID = "pair_device_title";

// Digits of the pairing check the phone shows (SAS_MODULUS 1e6 in remote/crypto.rs).
export const SAS_LENGTH = 6;

// Pairing codes live 2 minutes on the desktop (PAIRING_TTL in remote/pairing.rs).
export const PAIRING_TTL_MS = 120_000;

// The QR arrives as SVG markup; it is shown through an <img> data URL so the markup can never run
// script in the webview (img-src allows data:).
export const SVG_DATA_URL_PREFIX = "data:image/svg+xml;charset=utf-8,";

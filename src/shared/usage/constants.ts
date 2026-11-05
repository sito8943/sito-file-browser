// App id in the usage service's `APPS` list (sito-feature-usage).
export const USAGE_APP = "file-browser";

// Build-time endpoint and public app key. An empty endpoint turns tracking off (local builds
// without a .env, forks), so the app never depends on the service.
export const USAGE_URL = import.meta.env.VITE_USAGE_URL ?? "";
export const USAGE_KEY = import.meta.env.VITE_USAGE_KEY ?? "";

// Where the user's choice lives in the webview's local storage. The choice is per device and not
// part of settings.toml: it decides whether the webview may talk to the usage service at all.
export const CONSENT_STORAGE_KEY = "sfb:consent";

// Bump when the notice text or what is counted changes: older choices are asked again.
export const CONSENT_VERSION = 1;

// Feature-name prefixes. Every counted feature is `<prefix>.<snake_case id>`.
export const USAGE_PREFIX = {
  PAGE: "page",
  ACTION: "action",
  HOTKEY: "hotkey",
  SETTING: "setting",
} as const;

export const DATA_CHOICES_TITLE_ID = "data_choices_title";

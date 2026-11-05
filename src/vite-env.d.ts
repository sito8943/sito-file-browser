/// <reference types="vite/client" />

interface ImportMetaEnv {
  // Anonymous usage service (sito-feature-usage). Unset in local builds: tracking stays off.
  readonly VITE_USAGE_URL?: string;
  readonly VITE_USAGE_KEY?: string;
}

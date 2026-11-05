import type { HotkeyHandler } from "@sito/commands";

import { CONSENT_VERSION } from "./constants";
import type { StoredConsent, UsagePrefix } from "./types";

// `showHidden` → `show_hidden`: the service only accepts lowercase dotted slugs.
export const toSnakeCase = (id: string) =>
  id.replace(/[A-Z]/g, (char) => `_${char.toLowerCase()}`);

export const usageFeature = (prefix: UsagePrefix, id: string) =>
  `${prefix}.${toSnakeCase(id)}`;

// The stored choice, or null when there is none, it is malformed, or it predates CONSENT_VERSION.
// Storage access can throw (disabled/cleared webview data), so failures read as "no choice".
export const readStoredConsent = (storageKey: string): StoredConsent | null => {
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return null;
    const { analytics, version, decidedAt } = parsed as Record<string, unknown>;
    if (
      typeof analytics !== "boolean" ||
      version !== CONSENT_VERSION ||
      typeof decidedAt !== "string"
    )
      return null;
    return { analytics, version, decidedAt };
  } catch {
    return null;
  }
};

export const persistConsent = (
  storageKey: string,
  analytics: boolean,
  now: Date = new Date(),
): StoredConsent => {
  const consent: StoredConsent = {
    analytics,
    version: CONSENT_VERSION,
    decidedAt: now.toISOString(),
  };
  try {
    localStorage.setItem(storageKey, JSON.stringify(consent));
  } catch {
    // Unwritable storage: the choice still applies for this session.
  }
  return consent;
};

// Counts `feature` after `handler` runs, unless it declined the key (returned false) so the
// dispatcher falls through to another handler — that one is the feature actually used.
export const withUsage =
  (
    handler: HotkeyHandler,
    feature: string,
    track: (feature: string) => void,
  ): HotkeyHandler =>
  (event) => {
    const result = handler(event);
    if (result !== false) track(feature);
    return result;
  };

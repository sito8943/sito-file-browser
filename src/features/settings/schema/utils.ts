import { DEFAULT_SETTINGS } from "@/shared/constants";
import { USAGE_PREFIX, usageFeature } from "@/shared/usage";

import { SETTINGS_SCHEMA } from "./schema";

// Settings rows bound to a real AppSettings field. Synthetic custom rows (Storage panel, guide
// replay, …) change nothing persistent, so they are not counted as settings.
const COUNTED_SETTING_KEYS: ReadonlySet<string> = new Set(
  SETTINGS_SCHEMA.map((descriptor) => descriptor.key).filter(
    (key) => key in DEFAULT_SETTINGS,
  ),
);

// One `setting.<key>` usage feature per counted row, for the usage catalogue.
export const settingUsageFeatures = (): string[] =>
  [...COUNTED_SETTING_KEYS].map((key) =>
    usageFeature(USAGE_PREFIX.SETTING, key),
  );

// The usage features a settings patch touches: only user-visible settings, never internal markers
// (onboardingSeen, lastSeenVersion) that the app writes itself.
export const settingUsageFeaturesOf = (patch: object): string[] =>
  Object.keys(patch)
    .filter((key) => COUNTED_SETTING_KEYS.has(key))
    .map((key) => usageFeature(USAGE_PREFIX.SETTING, key));

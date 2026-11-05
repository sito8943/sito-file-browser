import { ENTRY_ACTION } from "@/features/directory";
import { settingUsageFeatures, SETTINGS_SECTION } from "@/features/settings";
import { SIDEBAR_ACTION } from "@/features/sidebar";
import { VOLUME_ACTION } from "@/features/volumes";
import { KEYMAP_ACTION } from "@/shared/keymap";
import { USAGE_EVENT, USAGE_PREFIX, usageFeature } from "@/shared/usage";

import { ROUTES } from "../routes";
import { registryUsageFeatures } from "./utils";

// Every countable feature, sent to the service so it can report the ones never used. Built from
// the registries that define them, so a new route, menu action (entry, sidebar, volume),
// rebindable shortcut, settings row or settings section is in the catalogue without extra work.
// The rest are the named USAGE_EVENT features, counted at their choke points (the one function
// every trigger goes through). The service caps a catalogue at 500 names.
export const USAGE_FEATURES: readonly string[] = [
  ...Object.keys(ROUTES).map((key) => usageFeature(USAGE_PREFIX.PAGE, key)),
  ...registryUsageFeatures(USAGE_PREFIX.ACTION, ENTRY_ACTION),
  ...registryUsageFeatures(USAGE_PREFIX.HOTKEY, KEYMAP_ACTION),
  ...registryUsageFeatures(USAGE_PREFIX.SIDEBAR_MENU, SIDEBAR_ACTION),
  ...registryUsageFeatures(USAGE_PREFIX.VOLUMES_MENU, VOLUME_ACTION),
  ...registryUsageFeatures(USAGE_PREFIX.SETTINGS_SECTION, SETTINGS_SECTION),
  ...settingUsageFeatures(),
  ...Object.values(USAGE_EVENT).flatMap((group) => Object.values(group)),
];

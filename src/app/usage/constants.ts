import { ENTRY_ACTION } from "@/features/directory";
import { settingUsageFeatures } from "@/features/settings";
import { KEYMAP_ACTION } from "@/shared/keymap";
import { USAGE_PREFIX, usageFeature } from "@/shared/usage";

import { ROUTES } from "../routes";

// Every countable feature, built from the registries that define them so a new route, menu
// action, rebindable shortcut or settings row is in the catalogue without extra work.
export const USAGE_FEATURES: readonly string[] = [
  ...Object.keys(ROUTES).map((key) => usageFeature(USAGE_PREFIX.PAGE, key)),
  ...Object.values(ENTRY_ACTION).map((id) =>
    usageFeature(USAGE_PREFIX.ACTION, id),
  ),
  ...Object.values(KEYMAP_ACTION).map((id) =>
    usageFeature(USAGE_PREFIX.HOTKEY, id),
  ),
  ...settingUsageFeatures(),
];

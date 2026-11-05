import { useHotkeys as useCommandsHotkeys } from "@sito/commands/react";
import type { HotkeySpec } from "@sito/commands";

import {
  USAGE_PREFIX,
  usageFeature,
  useUsage,
  withUsage,
} from "@/shared/usage";

export type { HotkeySpec } from "@sito/commands";

// The @sito/commands hook, counting rebindable actions like useHotkey does.
export const useHotkeys = <
  TCommandId extends string,
  TScope extends string = string,
>(
  specs: HotkeySpec<TCommandId, TScope>[],
  options?: { when?: boolean },
) => {
  const { track } = useUsage();
  const counted = specs.map((spec) =>
    typeof spec.binding === "string"
      ? {
          ...spec,
          handler: withUsage(
            spec.handler,
            usageFeature(USAGE_PREFIX.HOTKEY, spec.binding),
            track,
          ),
        }
      : spec,
  );
  useCommandsHotkeys<TCommandId, TScope>(counted, options);
};

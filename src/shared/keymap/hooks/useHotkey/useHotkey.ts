import { useHotkey as useCommandsHotkey } from "@sito/commands/react";
import type { HotkeyHandler, HotkeyOptions, KeyBinding } from "@sito/commands";

import {
  USAGE_PREFIX,
  usageFeature,
  useUsage,
  withUsage,
} from "@/shared/usage";

// The @sito/commands hook, plus an anonymous count of rebindable actions (`hotkey.<action>`) so the
// usage report shows which shortcuts people rely on. Fixed keys (Escape, arrows) are not counted.
// Handlers live in a ref inside @sito/commands, so wrapping on every render does not re-register.
export const useHotkey = <
  TCommandId extends string,
  TScope extends string = string,
>(
  binding: TCommandId | KeyBinding,
  handler: HotkeyHandler,
  options?: HotkeyOptions<TScope>,
) => {
  const { track } = useUsage();
  const counted =
    typeof binding === "string"
      ? withUsage(handler, usageFeature(USAGE_PREFIX.HOTKEY, binding), track)
      : handler;
  useCommandsHotkey<TCommandId, TScope>(binding, counted, options);
};

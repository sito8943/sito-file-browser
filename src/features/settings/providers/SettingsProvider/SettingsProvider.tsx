import { useCallback, useMemo, useState } from "react";

import { DEFAULT_SETTINGS } from "@/shared/constants";
import { useUsage } from "@/shared/usage";
import type { AppSettings } from "@/shared/services/api";

import { SettingsContextProvider } from "./SettingsContext";
import type { SettingsProviderProps } from "./types";
import { SettingsManager } from "../../managers/SettingsManager";
import { useSettingsShortcut } from "../../hooks/useSettingsShortcut";
import SettingsDialog from "../../components/SettingsDialog";
import { settingUsageFeaturesOf } from "../../schema";

// Owns the settings dialog's open state, exposes open/close + the settings binding to the app, and
// wires the VS Code-style shortcut. Renders the dialog itself so it's self-contained.
export const SettingsProvider = ({
  children,
  settings,
  update,
}: SettingsProviderProps) => {
  const [visible, setVisible] = useState(false);
  const manager = useMemo(() => new SettingsManager(), []);

  const open = useCallback(() => setVisible(true), []);
  const close = useCallback(() => setVisible(false), []);

  useSettingsShortcut(open);

  // Every settings write made through this context comes from the user (dialog rows, onboarding
  // steps), so it doubles as the anonymous "which settings get changed" count.
  const { track } = useUsage();
  const trackedUpdate = useCallback(
    (patch: Partial<AppSettings>) => {
      settingUsageFeaturesOf(patch).forEach(track);
      update(patch);
    },
    [track, update],
  );

  return (
    <SettingsContextProvider
      value={{
        open,
        close,
        settings,
        update: trackedUpdate,
        defaults: DEFAULT_SETTINGS,
        manager,
      }}
    >
      {children}
      <SettingsDialog visible={visible} onClose={close} />
    </SettingsContextProvider>
  );
};

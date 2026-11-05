import { useCallback, useMemo, useState, type ReactNode } from "react";

import { USAGE_EVENT, useUsage } from "@/shared/usage";

import { ChangelogContext } from "./ChangelogContext";

// Owns the "what's new" dialog's open state (and which version it opens on). Sits above
// SettingsProvider so the settings dialog's "Show changelog" button can reach it; the dialog itself
// is mounted below (see ChangelogDialog) because it reads settings.
export const ChangelogProvider = ({ children }: { children: ReactNode }) => {
  const [visible, setVisible] = useState(false);
  const [version, setVersion] = useState<string | null>(null);

  // Both callers are user actions (the Settings button and the what's-new toast click); nothing
  // opens the changelog unprompted, so counting here covers every way in.
  const { track } = useUsage();
  const open = useCallback(
    (v?: string) => {
      track(USAGE_EVENT.APP.CHANGELOG_OPEN);
      setVersion(v ?? null);
      setVisible(true);
    },
    [track],
  );
  const close = useCallback(() => setVisible(false), []);

  const value = useMemo(
    () => ({ visible, version, open, close }),
    [visible, version, open, close],
  );
  return (
    <ChangelogContext.Provider value={value}>
      {children}
    </ChangelogContext.Provider>
  );
};

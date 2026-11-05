import { useCallback, useEffect, useState } from "react";

import { USAGE_EVENT, useUsage } from "@/shared/usage";

import { SIDEBAR_COLLAPSED_STORAGE_KEY } from "./constants";

// Sidebar collapsed/expanded preference, persisted across sessions.
export const useSidebarCollapsed = () => {
  const [collapsed, setCollapsed] = useState<boolean>(
    () => localStorage.getItem(SIDEBAR_COLLAPSED_STORAGE_KEY) === String(true),
  );

  useEffect(() => {
    localStorage.setItem(SIDEBAR_COLLAPSED_STORAGE_KEY, String(collapsed));
  }, [collapsed]);

  const { track } = useUsage();
  // Both directions count: expanding back is as much a use of the feature as collapsing.
  const toggle = useCallback(() => {
    track(USAGE_EVENT.SIDEBAR.COLLAPSE);
    setCollapsed((c) => !c);
  }, [track]);

  return { collapsed, toggle };
};

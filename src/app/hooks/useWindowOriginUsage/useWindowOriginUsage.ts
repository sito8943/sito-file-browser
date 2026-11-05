import { useEffect, useRef } from "react";

import { openedAtPathFromUrl } from "@/features/tabs";
import { USAGE_EVENT, useUsage } from "@/shared/usage";

// Counts a window that was opened on a path (Finder Sync, the sito-file-browser:// scheme, Open
// With / a drop on the Dock icon) once per window. The query param is read at mount and never
// changes, so the only re-count guard needed is per tracker: like usePageUsage, accepting the
// notice later swaps in a live tracker, and the window's origin should count then.
export const useWindowOriginUsage = () => {
  const { track } = useUsage();
  const countedWith = useRef<typeof track | null>(null);

  useEffect(() => {
    if (!openedAtPathFromUrl() || countedWith.current === track) return;
    countedWith.current = track;
    track(USAGE_EVENT.APP.WINDOW_FROM_URL);
  }, [track]);
};

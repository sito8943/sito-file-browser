import { useCallback, useRef } from "react";

import { USAGE_BURST_GAP_MS } from "../../constants";
import { useUsage } from "../../providers/UsageContext";

// Returns a callback that counts `feature` once per burst of calls: a call counts only when the
// previous one was more than USAGE_BURST_GAP_MS ago. For continuous inputs (wheel zoom, a typed
// prefix) where every event is the same single use of the feature.
export const useTrackBurst = (feature: string) => {
  const { track } = useUsage();
  const lastCallAt = useRef(0);
  return useCallback(() => {
    const now = Date.now();
    if (now - lastCallAt.current > USAGE_BURST_GAP_MS) track(feature);
    lastCallAt.current = now;
  }, [feature, track]);
};

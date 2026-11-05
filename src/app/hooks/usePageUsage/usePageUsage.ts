import { useEffect, useRef } from "react";

import { useUsage } from "@/shared/usage";

import { pageUsageFeatureOf } from "./utils";

// Counts one view of the current screen (Volumes / Directory) each time the route changes. The
// ref keeps StrictMode's double effect run, and re-renders on the same route, from counting twice.
// It also remembers which tracker counted: accepting the notice swaps in a live tracker, and the
// screen the user is on must count then, not only after the next route change.
export const usePageUsage = (pathname: string) => {
  const { track } = useUsage();
  const last = useRef<{ feature: string; track: typeof track } | null>(null);
  const feature = pageUsageFeatureOf(pathname);

  useEffect(() => {
    if (!feature) return;
    if (last.current?.feature === feature && last.current.track === track)
      return;
    last.current = { feature, track };
    track(feature);
  }, [feature, track]);
};

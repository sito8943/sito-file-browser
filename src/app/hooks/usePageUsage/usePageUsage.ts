import { useEffect, useRef } from "react";

import { useUsage } from "@/shared/usage";

import { pageUsageFeatureOf } from "./utils";

// Counts one view of the current screen (Volumes / Directory) each time the route changes. The
// ref keeps StrictMode's double effect run, and re-renders on the same route, from counting twice.
export const usePageUsage = (pathname: string) => {
  const { track } = useUsage();
  const last = useRef<string | null>(null);
  const feature = pageUsageFeatureOf(pathname);

  useEffect(() => {
    if (!feature || last.current === feature) return;
    last.current = feature;
    track(feature);
  }, [feature, track]);
};

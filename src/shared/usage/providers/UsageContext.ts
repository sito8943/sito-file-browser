import { createContext, useContext } from "react";

import type { UsageContextValue } from "../types";

// Outside the provider (detached preview/properties windows) nothing is counted and no notice is
// shown. This is why useUsage does not throw like the other domain hooks: shared hotkey wrappers
// call it from every window.
const NO_USAGE: UsageContextValue = {
  analytics: null,
  isNoticeOpen: false,
  setAnalytics: () => {},
  track: () => {},
};

export const UsageContext = createContext<UsageContextValue>(NO_USAGE);

export const useUsage = () => useContext(UsageContext);

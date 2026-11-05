import type { ReactNode } from "react";

import type { USAGE_PREFIX } from "./constants";

export type UsagePrefix = (typeof USAGE_PREFIX)[keyof typeof USAGE_PREFIX];

// The user's choice about anonymous usage counts, kept as proof of consent.
export type StoredConsent = {
  analytics: boolean;
  version: number;
  // ISO date of the choice.
  decidedAt: string;
};

export type UsageContextValue = {
  // null until the user chooses.
  analytics: boolean | null;
  // The notice is shown: no choice yet, or the user reopened it.
  isNoticeOpen: boolean;
  setAnalytics: (allowed: boolean) => void;
  // Counts one use of a catalogue feature. A no-op without consent or endpoint.
  track: (feature: string) => void;
};

export type UsageProviderProps = {
  // Every countable feature, so the service can report the ones never used.
  features: readonly string[];
  children: ReactNode;
};

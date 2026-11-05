import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  createTracker,
  fetchTransport,
  noopTracker,
  type Tracker,
} from "@sito/feature-usage";

import {
  CONSENT_STORAGE_KEY,
  USAGE_APP,
  USAGE_KEY,
  USAGE_URL,
} from "../constants";
import type { UsageContextValue, UsageProviderProps } from "../types";
import { persistConsent, readStoredConsent } from "../utils";
import { UsageContext } from "./UsageContext";

// Owns the consent choice and the tracker. Nothing is sent until the user accepts in the notice
// (or in Settings); withdrawing stops the tracker and forgets the events it still held.
export const UsageProvider = ({ features, children }: UsageProviderProps) => {
  const [analytics, setAnalyticsState] = useState<boolean | null>(
    () => readStoredConsent(CONSENT_STORAGE_KEY)?.analytics ?? null,
  );
  const allowed = analytics === true;

  // fetch, not sendBeacon: the Tauri webview has no reliable beacon on window close, and queued
  // events survive in storage until the next launch anyway.
  const tracker = useMemo<Tracker<string>>(
    () =>
      allowed
        ? createTracker({
            app: USAGE_APP,
            key: USAGE_KEY,
            endpoint: USAGE_URL,
            features,
            transport: fetchTransport(),
            onError: (error: Error) => console.warn("[usage]", error.message),
          })
        : noopTracker<string>(),
    [allowed, features],
  );

  // Stop the tracker being replaced and, on withdrawal, clear what it kept. Not an effect
  // cleanup: StrictMode's extra run would stop the live tracker.
  const previous = useRef<Tracker<string> | null>(null);
  useEffect(() => {
    const replaced = previous.current;
    previous.current = tracker;
    if (!replaced || replaced === tracker) return;
    replaced.stop();
    if (!allowed) replaced.clear();
  }, [tracker, allowed]);

  // Other browser windows share this storage: follow a choice made there instead of asking again.
  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key !== CONSENT_STORAGE_KEY) return;
      setAnalyticsState(
        readStoredConsent(CONSENT_STORAGE_KEY)?.analytics ?? null,
      );
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const setAnalytics = useCallback((next: boolean) => {
    persistConsent(CONSENT_STORAGE_KEY, next);
    setAnalyticsState(next);
  }, []);

  const value = useMemo<UsageContextValue>(
    () => ({
      analytics,
      isNoticeOpen: analytics === null,
      setAnalytics,
      track: tracker.track,
    }),
    [analytics, setAnalytics, tracker],
  );

  return (
    <UsageContext.Provider value={value}>{children}</UsageContext.Provider>
  );
};

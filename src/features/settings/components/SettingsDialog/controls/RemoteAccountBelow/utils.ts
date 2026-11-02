import type { RemoteState } from "@/shared/services/api";
import { t } from "@/lang";

// Localized label for a remote service state.
export const stateLabel = (state: RemoteState): string =>
  t.remoteControl.states[state];

import type {
  RemoteAccount,
  RemotePairedDevice,
  RemotePendingDevice,
  RemoteRoot,
  RemoteStatus,
} from "@/shared/services/api";

import type { RemoteManager } from "../../managers/RemoteManager";

export type RemoteContextValue = {
  manager: RemoteManager;
  status: RemoteStatus;
  // undefined while the first lookup (which may restore the session from the keyring) runs.
  account: RemoteAccount | null | undefined;
  roots: RemoteRoot[] | null;
  devices: RemotePairedDevice[] | null;
  // A phone that proved the pairing code and waits for the SAS confirmation.
  pending: RemotePendingDevice | null;
  pairingVisible: boolean;
  // Resolve to true on success; failures are reported as toasts.
  signIn: (email: string, password: string) => Promise<boolean>;
  signOut: () => Promise<void>;
  addRoot: (path: string) => Promise<boolean>;
  removeRoot: (id: string) => Promise<void>;
  revokeDevice: (id: string) => Promise<void>;
  openPairing: () => void;
  // Closing also cancels the pairing window and any pending phone on the desktop.
  closePairing: () => void;
  confirmPairing: () => Promise<void>;
};

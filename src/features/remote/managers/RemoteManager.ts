import * as api from "@/shared/services/api";
import type {
  RemoteAccount,
  RemotePairedDevice,
  RemotePairingCode,
  RemotePendingDevice,
  RemoteRoot,
  RemoteSessionOpened,
  RemoteStatus,
} from "@/shared/services/api";

// Remote-control domain operations over the Tauri boundary. Instantiated by RemoteProvider; UI
// reaches it only through useRemote(). Every rule (allowlist, root confinement, pairing) lives in
// Rust; this class only forwards.
export class RemoteManager {
  status(): Promise<RemoteStatus> {
    return api.getRemoteStatus();
  }

  account(): Promise<RemoteAccount | null> {
    return api.getRemoteAccount();
  }

  signIn(email: string, password: string): Promise<RemoteAccount> {
    return api.remoteSignIn(email, password);
  }

  signOut(): Promise<void> {
    return api.remoteSignOut();
  }

  roots(): Promise<RemoteRoot[]> {
    return api.getRemoteRoots();
  }

  addRoot(path: string): Promise<RemoteRoot> {
    return api.addRemoteRoot(path);
  }

  removeRoot(id: string): Promise<void> {
    return api.removeRemoteRoot(id);
  }

  devices(): Promise<RemotePairedDevice[]> {
    return api.getRemoteDevices();
  }

  revokeDevice(id: string): Promise<void> {
    return api.revokeRemoteDevice(id);
  }

  startPairing(): Promise<RemotePairingCode> {
    return api.startRemotePairing();
  }

  pendingPairing(): Promise<RemotePendingDevice | null> {
    return api.getRemotePendingPairing();
  }

  confirmPairing(deviceId: string, code: string): Promise<RemotePairedDevice> {
    return api.confirmRemotePairing(deviceId, code);
  }

  cancelPairing(): Promise<void> {
    return api.cancelRemotePairing();
  }

  // Open a folder in a new file browser window (shared-folder chips).
  openPath(path: string): Promise<void> {
    return api.openPathInNewWindow(path);
  }

  onStatus(listener: (status: RemoteStatus) => void): Promise<() => void> {
    return api.onRemoteStatus(listener);
  }

  onPairing(
    listener: (device: RemotePendingDevice) => void,
  ): Promise<() => void> {
    return api.onRemotePairing(listener);
  }

  onSessionOpened(
    listener: (session: RemoteSessionOpened) => void,
  ): Promise<() => void> {
    return api.onRemoteSessionOpened(listener);
  }
}

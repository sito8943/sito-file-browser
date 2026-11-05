import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import type {
  RemoteAccount,
  RemotePairedDevice,
  RemotePendingDevice,
  RemoteRoot,
  RemoteStatus,
} from "@/shared/services/api";
import { notify, TOAST_TYPE } from "@/shared/toast";
import { t } from "@/lang";
import { USAGE_EVENT, useUsage } from "@/shared/usage";

import { RemoteManager } from "../../managers/RemoteManager";
import { REMOTE_STATE } from "../../constants";
import { remoteErrorMessage } from "../../utils";
import { RemoteContext } from "./RemoteContext";

const INITIAL_STATUS: RemoteStatus = {
  state: REMOTE_STATE.OFF,
  reason: null,
  online: [],
  sessions: [],
};

const SIGNED_OUT_REASON = "REMOTE_AUTH_SIGNED_OUT";

// Owns remote-control state for the Settings controls, the pairing dialog and the active-session
// indicator. Mirrors the Rust service through its events (status, pairing, session opened) and
// re-reads lists after each change. Sits above SettingsProvider so its controls can reach it.
export const RemoteProvider = ({ children }: { children: ReactNode }) => {
  const manager = useMemo(() => new RemoteManager(), []);
  const [status, setStatus] = useState<RemoteStatus>(INITIAL_STATUS);
  const [account, setAccount] = useState<RemoteAccount | null | undefined>();
  const [roots, setRoots] = useState<RemoteRoot[] | null>(null);
  const [devices, setDevices] = useState<RemotePairedDevice[] | null>(null);
  const [pending, setPending] = useState<RemotePendingDevice | null>(null);
  const [pairingVisible, setPairingVisible] = useState(false);
  // Phone-access features count on success only (a failed sign-in or revoke is not a use).
  // Aliased: the subscription effect below already has a local `track` helper.
  const { track: trackUsage } = useUsage();

  const fail = useCallback((error: unknown) => {
    notify(remoteErrorMessage(error), TOAST_TYPE.ERROR);
  }, []);

  const loadRoots = useCallback(
    () => manager.roots().then(setRoots).catch(fail),
    [manager, fail],
  );
  const loadDevices = useCallback(
    () => manager.devices().then(setDevices).catch(fail),
    [manager, fail],
  );

  useEffect(() => {
    let disposed = false;
    const unlisteners: (() => void)[] = [];
    const track = (subscription: Promise<() => void>) =>
      void subscription.then((unlisten) => {
        if (disposed) unlisten();
        else unlisteners.push(unlisten);
      });

    track(
      manager.onStatus((next) => {
        setStatus(next);
        // The service stopped because the sito-auth session died: reflect it in the account row.
        if (next.reason === SIGNED_OUT_REASON) setAccount(null);
      }),
    );
    track(manager.onPairing(setPending));
    track(
      manager.onSessionOpened((session) =>
        notify(
          t.remoteControl.sessionOpened(
            session.name || t.remoteControl.unknownPhone,
          ),
          TOAST_TYPE.INFO,
        ),
      ),
    );

    void manager.status().then((next) => {
      if (!disposed) setStatus(next);
    });
    // A failed lookup (offline, keychain locked) reads as signed out; the error is not actionable.
    void manager
      .account()
      .catch(() => null)
      .then((next) => {
        if (!disposed) setAccount(next);
      });
    void loadRoots();
    void loadDevices();
    void manager.pendingPairing().then((next) => {
      if (!disposed && next) setPending(next);
    });

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [manager, loadRoots, loadDevices]);

  const signIn = useCallback(
    async (email: string, password: string) => {
      try {
        setAccount(await manager.signIn(email, password));
        trackUsage(USAGE_EVENT.REMOTE.SIGN_IN);
        return true;
      } catch (error) {
        fail(error);
        return false;
      }
    },
    [manager, fail, trackUsage],
  );

  const signOut = useCallback(async () => {
    try {
      await manager.signOut();
      trackUsage(USAGE_EVENT.REMOTE.SIGN_OUT);
    } catch (error) {
      fail(error);
    }
    setAccount(null);
  }, [manager, fail, trackUsage]);

  const addRoot = useCallback(
    async (path: string) => {
      try {
        await manager.addRoot(path);
        trackUsage(USAGE_EVENT.REMOTE.ROOT_ADD);
        await loadRoots();
        return true;
      } catch (error) {
        fail(error);
        return false;
      }
    },
    [manager, fail, loadRoots, trackUsage],
  );

  const removeRoot = useCallback(
    async (id: string) => {
      try {
        await manager.removeRoot(id);
        trackUsage(USAGE_EVENT.REMOTE.ROOT_REMOVE);
      } catch (error) {
        fail(error);
      }
      await loadRoots();
    },
    [manager, fail, loadRoots, trackUsage],
  );

  const revokeDevice = useCallback(
    async (id: string) => {
      try {
        await manager.revokeDevice(id);
        trackUsage(USAGE_EVENT.REMOTE.DEVICE_REVOKE);
      } catch (error) {
        fail(error);
      }
      await loadDevices();
    },
    [manager, fail, loadDevices, trackUsage],
  );

  const openPairing = useCallback(() => {
    trackUsage(USAGE_EVENT.REMOTE.PAIR_OPEN);
    setPending(null);
    setPairingVisible(true);
  }, [trackUsage]);

  const closePairing = useCallback(() => {
    trackUsage(USAGE_EVENT.REMOTE.PAIR_CANCEL);
    setPairingVisible(false);
    setPending(null);
    void manager.cancelPairing().catch(fail);
  }, [manager, fail, trackUsage]);

  const confirmPairing = useCallback(
    async (code: string) => {
      if (!pending) return;
      try {
        const device = await manager.confirmPairing(pending.deviceId, code);
        trackUsage(USAGE_EVENT.REMOTE.PAIR_CONFIRM);
        notify(t.remoteControl.paired(device.name), TOAST_TYPE.SUCCESS);
        await loadDevices();
      } catch (error) {
        // One attempt: the backend already rejected the phone on a wrong code.
        fail(error);
      } finally {
        setPending(null);
        setPairingVisible(false);
      }
    },
    [manager, pending, fail, loadDevices, trackUsage],
  );

  const value = useMemo(
    () => ({
      manager,
      status,
      account,
      roots,
      devices,
      pending,
      pairingVisible,
      signIn,
      signOut,
      addRoot,
      removeRoot,
      revokeDevice,
      openPairing,
      closePairing,
      confirmPairing,
    }),
    [
      manager,
      status,
      account,
      roots,
      devices,
      pending,
      pairingVisible,
      signIn,
      signOut,
      addRoot,
      removeRoot,
      revokeDevice,
      openPairing,
      closePairing,
      confirmPairing,
    ],
  );

  return (
    <RemoteContext.Provider value={value}>{children}</RemoteContext.Provider>
  );
};

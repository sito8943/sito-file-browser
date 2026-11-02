import { lazy } from "react";

import { MountOnce } from "@/shared/components/patterns/Deferred";

import { useRemote } from "../../providers/RemoteProvider";

const PairDeviceDialogView = lazy(() => import("./PairDeviceDialog"));

// Always-mounted host: the pairing dialog chunk loads only the first time it opens.
const PairDeviceDialogHost = () => {
  const { pairingVisible } = useRemote();
  return (
    <MountOnce when={pairingVisible}>
      <PairDeviceDialogView />
    </MountOnce>
  );
};

export default PairDeviceDialogHost;

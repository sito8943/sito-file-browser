import { REMOTE_STATE, useRemote } from "@/features/remote";
import { t } from "@/lang";

import { useSettings } from "../../../providers/SettingsProvider";
import SettingsButton from "../SettingsButton";

// "Pair phone": closes Settings first (stacked modals fight for the MODAL scope) and opens the
// pairing dialog. Needs the service online, since the phone reaches this desktop through the relay.
const RemoteDevicesControl = () => {
  const { close } = useSettings();
  const { status, openPairing } = useRemote();
  const online = status.state === REMOTE_STATE.ONLINE;
  const onClick = () => {
    close();
    openPairing();
  };
  return (
    <SettingsButton
      onClick={onClick}
      disabled={!online}
      title={online ? undefined : t.remoteControl.pairNeedsOnline}
    >
      {t.remoteControl.pairButton}
    </SettingsButton>
  );
};

export default RemoteDevicesControl;

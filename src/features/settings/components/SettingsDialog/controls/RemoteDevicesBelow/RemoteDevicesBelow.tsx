import { useConfirm } from "@/shared/providers/ConfirmProvider";
import { useRemote } from "@/features/remote";
import { t } from "@/lang";

import SettingsButton from "../../SettingsButton";
import { deviceActivity } from "./utils";

// Paired phones, each with its live state (online / browsing) and a confirmed Revoke.
const RemoteDevicesBelow = () => {
  const { devices, status, revokeDevice } = useRemote();
  const { confirm } = useConfirm();
  if (devices === null) return null;
  if (devices.length === 0)
    return (
      <div className="settings_row_below">
        <span className="settings_row_hint">
          {t.remoteControl.devicesEmpty}
        </span>
      </div>
    );

  const revoke = async (id: string, name: string) => {
    const ok = await confirm({
      title: t.remoteControl.deviceRevokeTitle,
      message: t.remoteControl.deviceRevokeMessage(name),
      confirmLabel: t.remoteControl.deviceRevoke,
      destructive: true,
    });
    if (ok) await revokeDevice(id);
  };

  return (
    <ul className="settings_remote_devices">
      {devices.map((device) => {
        const activity = deviceActivity(device.id, status);
        return (
          <li key={device.id} className="settings_remote_device">
            <span className="settings_remote_device_name">{device.name}</span>
            {activity && <span className="settings_row_hint">{activity}</span>}
            <SettingsButton onClick={() => void revoke(device.id, device.name)}>
              {t.remoteControl.deviceRevoke}
            </SettingsButton>
          </li>
        );
      })}
    </ul>
  );
};

export default RemoteDevicesBelow;

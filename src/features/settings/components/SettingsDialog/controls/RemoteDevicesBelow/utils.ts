import type { RemoteStatus } from "@/shared/services/api";
import { t } from "@/lang";

// What a paired phone is doing right now, or null when it is offline.
export const deviceActivity = (
  id: string,
  status: RemoteStatus,
): string | null => {
  if (status.sessions.includes(id)) return t.remoteControl.deviceConnected;
  if (status.online.some((phone) => phone.id === id))
    return t.remoteControl.deviceOnline;
  return null;
};

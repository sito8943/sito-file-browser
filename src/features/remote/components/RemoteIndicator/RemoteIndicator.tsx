import { faMobileScreen } from "@fortawesome/free-solid-svg-icons";

import Icon from "@/shared/components/elements/Icon";
import { t } from "@/lang";

import "@/styles/components/RemoteIndicator.css";

import { useRemote } from "../../providers/RemoteProvider";

// Always-visible notice while a paired phone has an open session (plan: "indicador visible cuando
// hay una sesión remota activa"). Static, so fill-only (no border); renders nothing otherwise.
const RemoteIndicator = () => {
  const { status } = useRemote();
  const count = status.sessions.length;
  if (count === 0) return null;
  return (
    <div className="RemoteIndicator" role="status">
      <Icon icon={faMobileScreen} />
      <span>{t.remoteControl.indicator(count)}</span>
    </div>
  );
};

export default RemoteIndicator;

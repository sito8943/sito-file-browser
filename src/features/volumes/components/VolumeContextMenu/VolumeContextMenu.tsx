import {
  ContextMenu,
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/shared/components/patterns/ContextMenu";
import Icon from "@/shared/components/elements/Icon";
import { useStateContext } from "@/shared/providers/StateProvider";
import { USAGE_PREFIX, usageFeature, useUsage } from "@/shared/usage";

import {
  VOLUME_ACTIONS,
  VOLUME_MENU,
  ACTION_SEPARATOR,
  type VolumeActionContext,
  type VolumeActionId,
} from "../../actions";

import type { VolumeContextMenuProps } from "./types";

// Right-click menu for the Volumes view. The actions come from the declarative VOLUME_MENU; each
// id resolves to a predefined descriptor that owns its label, icon and behavior (Eject only shows
// for removable volumes via its isVisible predicate).
const VolumeContextMenu = ({
  contextMenuRef,
  visible,
  volume,
  onClose,
  openProperties,
}: VolumeContextMenuProps) => {
  const { fs, setPath, newTab, setVolumes } = useStateContext();
  const { track } = useUsage();

  const ctx: VolumeActionContext | null = volume
    ? {
        path: volume.mountPoint,
        name: volume.name,
        isEjectable: volume.isEjectable,
        fs,
        setPath,
        openInNewTab: newTab,
        openProperties,
        refreshVolumes: () => {
          fs.listVolumes().then(setVolumes);
        },
        onClose,
      }
    : null;

  return (
    <ContextMenu contextMenuVisible={visible} ref={contextMenuRef}>
      {ctx &&
        VOLUME_MENU.map((id, index) => {
          if (id === ACTION_SEPARATOR)
            return <ContextMenuSeparator key={`separator-${index}`} />;

          const action = VOLUME_ACTIONS[id as VolumeActionId];
          if (!action || (action.isVisible && !action.isVisible(ctx)))
            return null;

          return (
            <ContextMenuItem
              key={action.id}
              leading={<Icon icon={action.icon} />}
              onClick={() => {
                track(usageFeature(USAGE_PREFIX.VOLUMES_MENU, action.id));
                action.run(ctx);
              }}
            >
              {action.label()}
            </ContextMenuItem>
          );
        })}
    </ContextMenu>
  );
};

export default VolumeContextMenu;

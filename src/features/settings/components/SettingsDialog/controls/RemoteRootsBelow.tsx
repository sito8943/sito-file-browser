import Button from "@/shared/components/elements/Button";
import DeletableChip from "@/shared/components/elements/DeletableChip";
import { useRemote } from "@/features/remote";
import { t } from "@/lang";

// The shared folders as chips: the name opens the folder in a new window (full path as tooltip),
// × stops sharing it at once, also for phones already browsing it.
const RemoteRootsBelow = () => {
  const { roots, removeRoot, manager } = useRemote();
  if (roots === null) return null;
  if (roots.length === 0)
    return (
      <div className="settings_row_below">
        <span className="settings_row_hint">{t.remoteControl.rootsEmpty}</span>
      </div>
    );
  return (
    <div className="settings_ignores_chips settings_remote_list">
      {roots.map((root) => (
        <DeletableChip
          key={root.id}
          removeLabel={t.remoteControl.rootRemove}
          onDelete={() => void removeRoot(root.id)}
        >
          <Button
            unstyled
            className="settings_remote_root_name text_link"
            title={root.path}
            onClick={() => void manager.openPath(root.path)}
          >
            {root.name}
          </Button>
        </DeletableChip>
      ))}
    </div>
  );
};

export default RemoteRootsBelow;

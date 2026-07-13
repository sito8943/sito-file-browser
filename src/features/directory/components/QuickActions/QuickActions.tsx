import { useStateContext } from "@/shared/providers/StateProvider";
import { useKeymap, formatBinding } from "@/shared/keymap";
import IconButton from "@/shared/components/elements/IconButton";
import { extension } from "@/shared/utils";
import { RECENTS, TRASH_DIR_NAME } from "@/shared/constants";
import { ENTRY_KIND, opensInAppPreview } from "@/features/directory/constants";

import { useDirectory } from "../../providers/DirectoryProvider";
import { useContextMenuLayout } from "../../hooks/useContextMenuLayout";
import { useArchiveActions } from "../../hooks/useArchiveActions";
import { useSevenzipAvailable } from "@/shared/hooks/useSevenzipAvailable";
import {
  ENTRY_ACTION,
  ENTRY_ACTIONS,
  ACTION_SEPARATOR,
  resolveActionIds,
  isActionVisible,
  resolveActionIcon,
  type EntryActionContext,
  type EntryActionId,
} from "../../actions";

import QuickActionMenu from "./QuickActionMenu";

import "@/styles/components/QuickActions.css";

// Quick-actions toolbar (left of the QuickBar): Sort always targets the folder being viewed; the
// remaining context-menu actions target that folder when nothing is selected, or the selection.
const QuickActions = () => {
  const {
    fs,
    path,
    setPath,
    newTab,
    showHidden,
    toggleShowHidden,
    previewImagesInApp,
    previewMarkdownInApp,
  } = useStateContext();
  const { keymap } = useKeymap();
  const layout = useContextMenuLayout();
  const {
    sorted,
    selectedIDs,
    setRenamingID,
    fileOps,
    preview,
    properties,
    searchActive,
    sort,
    handleSort,
  } = useDirectory();
  const { onCompress, onExtract, onExtractToFolder } =
    useArchiveActions(fileOps);
  const sevenzipAvailable = useSevenzipAvailable();

  const hasSelection = selectedIDs.length > 0;
  const elementId = hasSelection ? selectedIDs[0] : path;
  const firstEntry = hasSelection
    ? sorted.find((entry) => entry.path === selectedIDs[0])
    : undefined;
  const elementType =
    hasSelection && firstEntry && !firstEntry.metadata.isDir
      ? ENTRY_KIND.FILE
      : ENTRY_KIND.DIRECTORY;
  const isCurrentDirectory = !hasSelection;
  const inTrash = path.endsWith(`/${TRASH_DIR_NAME}`);
  const fileExtension = extension(elementId);

  const ctx: EntryActionContext = {
    elementId,
    elementType,
    targets: hasSelection ? selectedIDs : [path],
    isCurrentDirectory,
    isDispersedView: path === RECENTS || searchActive,
    canPaste: !!fileOps.clipboard,
    fs,
    fileOps,
    setPath,
    openInNewTab: newTab,
    onClose: () => {},
    onStartRename: setRenamingID,
    onPreview: preview.open,
    onProperties: properties.open,
    onCompress,
    onExtract,
    onExtractToFolder,
    sevenzipAvailable,
    sort,
    onSort: handleSort,
    showHidden,
    toggleShowHidden,
    opensInAppPreview: opensInAppPreview(
      fileExtension.toLowerCase(),
      previewImagesInApp,
      previewMarkdownInApp,
    ),
  };

  // Sort always belongs to the folder being viewed, not to the current selection. Keep its Quick
  // Bar menu available while file/folder actions adapt to whatever the user has selected.
  const sortCtx: EntryActionContext = {
    ...ctx,
    elementId: path,
    elementType: ENTRY_KIND.DIRECTORY,
    targets: [path],
    isCurrentDirectory: true,
    isDispersedView: false,
    opensInAppPreview: false,
  };
  const sortAction = ENTRY_ACTIONS[ENTRY_ACTION.SORT_BY];

  const actionIds = resolveActionIds(layout, {
    isCurrentDirectory,
    inTrash,
    elementType,
    extension: fileExtension,
  }).filter(
    (id) => id !== ACTION_SEPARATOR && id !== ENTRY_ACTION.SORT_BY,
  );

  if (path === "") return null;

  return (
    <div className="quick_actions">
      <QuickActionMenu action={sortAction} ctx={sortCtx} />
      {actionIds.map((id) => {
        const action = ENTRY_ACTIONS[id as EntryActionId];
        if (!action || !isActionVisible(action, ctx)) return null;
        if (action.submenu)
          return <QuickActionMenu key={action.id} action={action} ctx={ctx} />;
        if (!action.run) return null;

        const enabled = action.isEnabled ? action.isEnabled(ctx) : true;
        const hotkey =
          action.hotkey ??
          (action.keymapAction
            ? formatBinding(keymap[action.keymapAction])
            : undefined);

        return (
          <IconButton
            key={action.id}
            icon={resolveActionIcon(action, ctx)}
            tooltip={action.label()}
            hotkey={hotkey}
            disabled={!enabled}
            onClick={() => action.run?.(ctx)}
            className={action.color ? `qa_${action.color}` : undefined}
          />
        );
      })}
    </div>
  );
};

export default QuickActions;

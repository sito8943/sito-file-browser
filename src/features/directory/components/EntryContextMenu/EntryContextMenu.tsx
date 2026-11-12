import {
  ContextMenu,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSubmenu,
  SUBMENU_MENU_CLASS,
  menuItemColor,
} from "@/shared/components/patterns/ContextMenu";
import Icon from "@/shared/components/elements/Icon";
import { useStateContext } from "@/shared/providers/StateProvider";
import { extension } from "@/shared/utils";
import { RECENTS } from "@/shared/constants";
import { useKeymap, formatBinding, isMacPlatform } from "@/shared/keymap";
import { customActionIcon } from "@/shared/contextActions";
import {
  USAGE_EVENT,
  USAGE_PREFIX,
  usageFeature,
  useUsage,
} from "@/shared/usage";

import { TagPicker } from "./TagPicker";

import {
  ENTRY_ACTIONS,
  ACTION_SEPARATOR,
  resolveActionIds,
  resolveCustomActions,
  isActionVisible,
  resolveActionIcon,
  type EntryActionContext,
  type EntryActionId,
} from "../../actions";
import { opensInAppPreview } from "../../constants";
import { useFileTypeExtensions } from "@/shared/formats";
import { useContextMenuLayout } from "../../hooks/useContextMenuLayout";
import { useArchiveActions } from "../../hooks/useArchiveActions";
import { useSevenzipAvailable } from "@/shared/hooks/useSevenzipAvailable";
import { useDirectory } from "../../providers/DirectoryProvider";

import type { EntryContextMenuProps } from "./types";

// Right-click menu for the directory: the current directory (empty area), a folder, or a
// file. The visible actions per context come from context_menu.toml (loaded once); each id
// resolves to a predefined action descriptor that owns its label, icon, hotkey and behavior.
const EntryContextMenu = ({
  contextMenuRef,
  visible,
  onClose,
  actionIds: providedActionIds,
  showTags = true,
  elementId,
  elementType,
  isCurrentDirectory,
  inTrash,
  selectedIDs,
  canPaste,
  fileOps,
  onStartRename,
  onPreview,
  onProperties,
}: EntryContextMenuProps) => {
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
  const { onCompress, onExtract, onExtractToFolder } =
    useArchiveActions(fileOps);
  const sevenzipAvailable = useSevenzipAvailable();
  const { sort, handleSort, searchActive, focusCreatedEntry } = useDirectory();
  const { keymap } = useKeymap();
  const { track } = useUsage();
  const layout = useContextMenuLayout();
  const fileTypes = useFileTypeExtensions();

  // Act on the whole selection if the clicked item is part of it, otherwise just that item.
  const targets = selectedIDs.includes(elementId) ? selectedIDs : [elementId];
  const fileExtension = extension(elementId);

  const ctx: EntryActionContext = {
    elementId,
    elementType,
    targets,
    isCurrentDirectory,
    isDispersedView: path === RECENTS || searchActive,
    canPaste,
    fs,
    fileOps,
    setPath,
    openInNewTab: newTab,
    onClose,
    onStartRename,
    onEntryCreated: focusCreatedEntry,
    onPreview,
    onProperties,
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
      fileTypes,
      previewImagesInApp,
      previewMarkdownInApp,
    ),
  };

  const actionIds =
    providedActionIds ??
    resolveActionIds(layout, {
      isCurrentDirectory,
      inTrash,
      elementType,
      extension: fileExtension,
    });
  // Reduced menus (path breadcrumbs) still expose applicable user-defined actions.
  const customActions = resolveCustomActions(layout, {
    isCurrentDirectory,
    inTrash,
    elementType,
    extension: fileExtension,
    elementId,
  });
  const hasVisiblePredefinedActions = actionIds.some((id) => {
    const action = ENTRY_ACTIONS[id as EntryActionId];
    return !!action && isActionVisible(action, ctx);
  });

  // Finder tags: a colour-swatch row at the top, for a real entry (not the empty-directory menu
  // or the Trash). macOS only — tags are a native macOS feature.
  const shouldShowTags =
    showTags && isMacPlatform() && !isCurrentDirectory && !inTrash;

  return (
    <ContextMenu contextMenuVisible={visible} ref={contextMenuRef}>
      {shouldShowTags && (
        <>
          <TagPicker targets={targets} onClose={onClose} />
          <ContextMenuSeparator />
        </>
      )}
      {actionIds.map((id, index) => {
        if (id === ACTION_SEPARATOR)
          return <ContextMenuSeparator key={`separator-${index}`} />;

        const action = ENTRY_ACTIONS[id as EntryActionId];
        if (!action || !isActionVisible(action, ctx)) return null;

        const enabled = action.isEnabled ? action.isEnabled(ctx) : true;
        const hotkey =
          action.hotkey ??
          (action.keymapAction
            ? formatBinding(keymap[action.keymapAction])
            : undefined);

        const leading = <Icon icon={resolveActionIcon(action, ctx)} />;

        // Submenu action (e.g. Sort By): its rows own their behavior; picking one closes the menu.
        if (action.submenu)
          return (
            <ContextMenuSubmenu
              key={action.id}
              label={action.label()}
              leading={leading}
              menuClassName={SUBMENU_MENU_CLASS}
            >
              {action.submenu(ctx).map((sub) =>
                sub.isSeparator ? (
                  <ContextMenuSeparator key={sub.key} />
                ) : (
                  <ContextMenuItem
                    key={sub.key}
                    leading={sub.icon ? <Icon icon={sub.icon} /> : undefined}
                    checked={sub.checked}
                    disabled={!sub.onClick}
                    onClick={
                      sub.onClick
                        ? () => {
                            track(usageFeature(USAGE_PREFIX.ACTION, action.id));
                            sub.onClick?.();
                            onClose();
                          }
                        : undefined
                    }
                  >
                    {sub.label}
                  </ContextMenuItem>
                ),
              )}
            </ContextMenuSubmenu>
          );

        const run = enabled ? action.run : undefined;

        return (
          <ContextMenuItem
            key={action.id}
            leading={leading}
            shortcut={hotkey}
            color={menuItemColor(action.color)}
            checked={action.checked?.(ctx)}
            disabled={!run}
            onClick={
              run
                ? () => {
                    track(usageFeature(USAGE_PREFIX.ACTION, action.id));
                    void run(ctx);
                  }
                : undefined
            }
          >
            {action.label()}
          </ContextMenuItem>
        );
      })}
      {customActions.length > 0 && hasVisiblePredefinedActions && (
        <ContextMenuSeparator />
      )}
      {customActions.map((action) => (
        <ContextMenuItem
          key={`custom-${action.id}`}
          leading={<Icon icon={customActionIcon(action.icon)} />}
          onClick={() => {
            track(USAGE_EVENT.FILES.CUSTOM_ACTION_RUN);
            onClose();
            void fs.runContextAction(action.id, elementId, targets);
          }}
        >
          {action.label}
        </ContextMenuItem>
      ))}
    </ContextMenu>
  );
};

export default EntryContextMenu;

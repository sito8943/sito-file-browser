import { useCallback, useEffect, useState } from "react";

import type { CustomContextAction, ContextMenuLayout } from "@/shared/models";
import { CONTEXT_ACTIONS_BUNDLE_EXTENSION } from "@/shared/contextActions";
import { useConfirm } from "@/shared/providers/ConfirmProvider";
import { useFilePicker } from "@/shared/providers/FilePickerProvider";
import { useFolderPicker } from "@/shared/providers/FolderPickerProvider";
import { notify, TOAST_TYPE } from "@/shared/toast";
import { t } from "@/lang";
import { USAGE_EVENT, useUsage } from "@/shared/usage";

import { useSettings } from "../../../../providers/SettingsProvider";

export const useContextActions = () => {
  const { manager } = useSettings();
  const { pickFolder } = useFolderPicker();
  const { pickFile } = useFilePicker();
  const { confirm } = useConfirm();
  const [layout, setLayout] = useState<ContextMenuLayout | null>(null);
  const [saving, setSaving] = useState(false);
  const { track } = useUsage();

  useEffect(() => {
    let cancelled = false;
    manager
      .getContextMenu()
      .then((next) => {
        if (!cancelled) setLayout(next);
      })
      .catch((error) =>
        notify(
          t.settings.contextActionsLoadError(String(error)),
          TOAST_TYPE.ERROR,
        ),
      );
    return () => {
      cancelled = true;
    };
  }, [manager]);

  // Uncounted writer shared by the editor's save and the import, so an import is counted as
  // custom_action.import only, not also as a save.
  const writeActions = useCallback(
    async (actions: CustomContextAction[]): Promise<boolean> => {
      if (!layout || saving) return false;
      setSaving(true);
      try {
        setLayout(
          await manager.setContextMenu({
            ...layout,
            custom_action: actions,
          }),
        );
        return true;
      } catch (error) {
        notify(
          t.settings.contextActionsSaveError(String(error)),
          TOAST_TYPE.ERROR,
        );
        return false;
      } finally {
        setSaving(false);
      }
    },
    [layout, manager, saving],
  );

  const save = useCallback(
    async (actions: CustomContextAction[]): Promise<boolean> => {
      const saved = await writeActions(actions);
      if (saved) track(USAGE_EVENT.APP.CUSTOM_ACTION_SAVE);
      return saved;
    },
    [writeActions, track],
  );

  const actions = layout?.custom_action ?? null;

  // Export: pick a destination folder and write context-actions.json there. An existing bundle is
  // only replaced once the user accepts the overwrite prompt, so a shared file is never clobbered
  // silently.
  const exportActions = useCallback(async () => {
    if (!actions || saving) return;
    if (actions.length === 0) {
      notify(t.settings.contextActionsExportEmpty, TOAST_TYPE.INFO);
      return;
    }
    const dir = await pickFolder();
    if (!dir) return;
    try {
      const first = await manager.exportContextActions(dir, actions, false);
      if (first.existed) {
        const accepted = await confirm({
          title: t.settings.contextActionsExport,
          message: t.settings.contextActionsExportExists,
          confirmLabel: t.settings.overwrite,
          destructive: true,
        });
        if (!accepted) return;
        await manager.exportContextActions(dir, actions, true);
      }
      track(USAGE_EVENT.APP.CUSTOM_ACTION_EXPORT);
      notify(t.settings.contextActionsExported(first.path), TOAST_TYPE.SUCCESS);
    } catch (error) {
      notify(
        t.settings.contextActionsExportError(String(error)),
        TOAST_TYPE.ERROR,
      );
    }
  }, [actions, saving, pickFolder, manager, confirm, track]);

  // Import: pick a bundle, validate it, and append its actions to the current list. Ids are
  // regenerated on parse, so importing never overwrites an action the user already has — a
  // duplicate lands as a second entry they can delete.
  const importActions = useCallback(async () => {
    if (!actions || saving) return;
    const file = await pickFile({
      extensions: [CONTEXT_ACTIONS_BUNDLE_EXTENSION],
    });
    if (!file) return;
    try {
      const imported = await manager.importContextActions(file, {
        invalid: t.settings.contextActionsImportInvalid,
        empty: t.settings.contextActionsImportEmpty,
      });
      if (await writeActions([...actions, ...imported])) {
        track(USAGE_EVENT.APP.CUSTOM_ACTION_IMPORT);
        notify(
          t.settings.contextActionsImported(imported.length),
          TOAST_TYPE.SUCCESS,
        );
      }
    } catch (error) {
      notify(
        t.settings.contextActionsImportError(
          error instanceof Error ? error.message : String(error),
        ),
        TOAST_TYPE.ERROR,
      );
    }
  }, [actions, saving, pickFile, manager, writeActions, track]);

  return {
    actions,
    saving,
    save,
    exportActions,
    importActions,
  };
};

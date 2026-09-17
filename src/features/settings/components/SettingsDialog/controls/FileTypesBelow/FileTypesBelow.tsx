import { type KeyboardEvent } from "react";
import { faPlus } from "@fortawesome/free-solid-svg-icons";

import DeletableChip from "@/shared/components/elements/DeletableChip";
import TextInput from "@/shared/components/elements/TextInput";
import Select from "@/shared/components/elements/Select";
import IconButton, {
  ICON_BUTTON_SIZE,
  ICON_BUTTON_VARIANT,
} from "@/shared/components/elements/IconButton";
import { KEY } from "@/shared/constants";
import { t } from "@/lang";

import type { CustomControlProps } from "../../../../schema";

import { categoryLabel } from "./constants";
import { useFileTypeEditor } from "./useFileTypeEditor";

// Full-width editor for the extension → file-type category map: pick a category, type an
// extension, and it takes that category's glyph, thumbnailing and preview. The user never picks a
// glyph — the category decides it — and an extension may only belong to one category.
const FileTypesBelow = (props: CustomControlProps) => {
  const {
    categories,
    extensions,
    category,
    setCategory,
    draft,
    changeDraft,
    duplicate,
    add,
    remove,
  } = useFileTypeEditor(props);

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === KEY.ENTER) {
      event.preventDefault();
      add();
    }
  };

  return (
    <div className="settings_file_types">
      <div className="settings_file_types_input">
        <Select
          className="settings_select"
          value={category}
          onChange={(event) =>
            setCategory(event.target.value as typeof category)
          }
          aria-label={t.settings.fileTypesCategoryLabel}
        >
          {categories.map((option) => (
            <option key={option} value={option}>
              {categoryLabel(option)}
            </option>
          ))}
        </Select>
        <TextInput
          className="settings_input"
          value={draft}
          placeholder={t.settings.fileTypesPlaceholder}
          onChange={(event) => changeDraft(event.target.value)}
          onKeyDown={onKeyDown}
          spellCheck={false}
          // eslint-disable-next-line i18next/no-literal-string -- HTML input attribute value
          autoCapitalize="off"
          // eslint-disable-next-line i18next/no-literal-string -- HTML input attribute value
          autoCorrect="off"
        />
        <IconButton
          icon={faPlus}
          variant={ICON_BUTTON_VARIANT.BOXED}
          size={ICON_BUTTON_SIZE.LG}
          onClick={add}
          disabled={!draft.trim()}
          tooltip={t.settings.fileTypesAdd}
          aria-label={t.settings.fileTypesAdd}
        />
      </div>
      {duplicate && (
        <span className="settings_file_types_error" role="alert">
          {t.settings.fileTypesDuplicate(
            duplicate.extension,
            categoryLabel(duplicate.category),
          )}
        </span>
      )}
      <div className="settings_file_types_groups">
        {categories.map((option) => (
          <div key={option} className="settings_file_types_group">
            <span className="settings_file_types_group_label">
              {categoryLabel(option)}
            </span>
            {(extensions[option] ?? []).length === 0 ? (
              <span className="settings_row_hint">
                {t.settings.fileTypesEmpty}
              </span>
            ) : (
              <div className="settings_file_types_chips">
                {extensions[option].map((ext: string) => (
                  <DeletableChip
                    key={ext}
                    removeLabel={t.settings.fileTypesRemove}
                    onDelete={() => remove(option, ext)}
                  >
                    {ext}
                  </DeletableChip>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};

export default FileTypesBelow;

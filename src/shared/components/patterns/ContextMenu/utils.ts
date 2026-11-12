import { CONTEXT_MENU_ITEM_COLORS, type ContextMenuItemColor } from "@sito/ui";

import { UI_COLOR, type UiColor } from "@/shared/constants";

// Action descriptors carry the app-wide UiColor (also used by the Quick Bar); context-menu rows
// take @sito/ui's item color, where a destructive row is "error".
export const menuItemColor = (color?: UiColor): ContextMenuItemColor =>
  color === UI_COLOR.DANGER
    ? CONTEXT_MENU_ITEM_COLORS.ERROR
    : CONTEXT_MENU_ITEM_COLORS.DEFAULT;

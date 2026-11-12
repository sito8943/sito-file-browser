// aria-haspopup value for controls that open a context menu.
export const MENU_ROLE = "menu";
// First focusable row of an open menu (plain and checkable items, see @sito/ui ContextMenuItem).
export const MENU_ITEM_SELECTOR =
  '[role="menuitem"]:not([disabled]), [role="menuitemcheckbox"]:not([disabled])';

export const CONTEXT_MENU_INITIAL_POSITION = { x: 0, y: 0 };

// Class names for a ContextMenuSubmenu flyout (`menuClassName`): the same surface as the main
// menu (.context_menu), plus the submenu tweaks in ContextMenu.css.
export const SUBMENU_MENU_CLASS = "context_menu context_menu_submenu visible";

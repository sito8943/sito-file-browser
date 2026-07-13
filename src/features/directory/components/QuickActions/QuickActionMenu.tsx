import type { MouseEvent } from "react";

import Icon from "@/shared/components/elements/Icon";
import IconButton from "@/shared/components/elements/IconButton";
import {
  ContextMenu,
  ContextMenuItem,
} from "@/shared/components/patterns/ContextMenu";

import { resolveActionIcon } from "../../actions";
import { useContextMenu } from "../../hooks/useContextMenu";

import type { QuickActionMenuProps } from "./types";

// Quick Bar button for an action whose choices live in a context-menu submenu (Sort By today).
// It renders those same descriptor-owned choices as a flat anchored menu instead of duplicating
// their labels, checked state, or behavior.
const QuickActionMenu = ({ action, ctx }: QuickActionMenuProps) => {
  const menu = useContextMenu();
  const items = action.submenu?.(ctx) ?? [];

  const openMenu = (event: MouseEvent<HTMLButtonElement>) => {
    const anchor = event.currentTarget.getBoundingClientRect();
    menu.openAt(anchor.left, anchor.bottom, ctx.elementId, ctx.elementType);
  };

  return (
    <>
      <IconButton
        icon={resolveActionIcon(action, ctx)}
        tooltip={action.label()}
        onClick={openMenu}
        disabled={action.isEnabled ? !action.isEnabled(ctx) : false}
        aria-haspopup="menu"
        aria-expanded={menu.visible}
        className={action.color ? `qa_${action.color}` : undefined}
      />
      <ContextMenu contextMenuVisible={menu.visible} ref={menu.ref}>
        {items.map((item) => (
          <ContextMenuItem
            key={item.key}
            isSeparator={item.isSeparator}
            text={item.label}
            icon={item.icon ? <Icon icon={item.icon} /> : undefined}
            checked={item.checked}
            onClick={
              item.onClick
                ? () => {
                    item.onClick?.();
                    menu.setVisible(false);
                  }
                : undefined
            }
          />
        ))}
      </ContextMenu>
    </>
  );
};

export default QuickActionMenu;

import { useCallback, type PointerEvent as ReactPointerEvent } from "react";

import { useStateContext } from "@/shared/providers/StateProvider";
import { USAGE_EVENT, useUsage } from "@/shared/usage";

import { SIDEBAR_RESIZING_CLASS } from "./constants";
import { clampWidth } from "./utils";

// Drives the sidebar's draggable right edge. The sidebar's left edge sits at viewport x=0, so the
// pointer's clientX is the target width. Width persists via setSidebarWidth (debounced to disk).
export const useSidebarResize = () => {
  const { sidebarWidth, setSidebarWidth } = useStateContext();
  const { track } = useUsage();

  const onPointerDown = useCallback(
    (event: ReactPointerEvent) => {
      event.preventDefault();
      document.body.classList.add(SIDEBAR_RESIZING_CLASS);

      // Counted once per drag, and only when the width ended up different: a click on the edge
      // without moving is not a resize.
      const startWidth = sidebarWidth;
      let lastWidth = startWidth;
      const onMove = (moveEvent: PointerEvent) => {
        lastWidth = clampWidth(moveEvent.clientX);
        setSidebarWidth(lastWidth);
      };

      const onUp = () => {
        if (lastWidth !== startWidth) track(USAGE_EVENT.SIDEBAR.RESIZE);
        document.body.classList.remove(SIDEBAR_RESIZING_CLASS);
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
      };

      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    },
    [sidebarWidth, setSidebarWidth, track],
  );

  return { onPointerDown };
};

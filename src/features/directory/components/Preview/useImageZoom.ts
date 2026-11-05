import { useCallback, useRef, useState } from "react";

import { USAGE_EVENT, useUsage } from "@/shared/usage";

import {
  IMAGE_ZOOM_MIN,
  IMAGE_ZOOM_MAX,
  NO_PAN,
  IMAGE_ROTATION_STEP,
  IMAGE_ROTATION_FULL,
} from "./constants";

const clamp = (value: number): number =>
  Math.min(IMAGE_ZOOM_MAX, Math.max(IMAGE_ZOOM_MIN, value));

// Image-preview zoom + pan state, owned by Preview so the zoom control can live in the shared
// bottom controls bar (next to prev/next) while ZoomableImage drives wheel/drag. Pan resets to
// centre whenever the zoom returns to 1x (done here, not in an effect).
export const useImageZoom = (filePath: string, visible: boolean) => {
  const [zoom, setZoom] = useState(IMAGE_ZOOM_MIN);
  const [pan, setPan] = useState(NO_PAN);
  const [rotation, setRotation] = useState(0);
  const [session, setSession] = useState({ filePath, visible });
  const { track } = useUsage();

  // Zoom is counted once per preview session (file shown), not per wheel tick or button press:
  // one wheel gesture fires dozens of zoom steps, which would drown the count. `session` gets a
  // new identity whenever the file or visibility changes, so comparing identities is the reset.
  const zoomCountedFor = useRef<typeof session | null>(null);
  const countZoom = useCallback(() => {
    if (zoomCountedFor.current === session) return;
    zoomCountedFor.current = session;
    track(USAGE_EVENT.PREVIEW.IMAGE_ZOOM);
  }, [session, track]);

  // File changes also come from deletion or reopening, not just the navigation buttons.
  if (session.filePath !== filePath || session.visible !== visible) {
    setSession({ filePath, visible });
    setZoom(IMAGE_ZOOM_MIN);
    setPan(NO_PAN);
    setRotation(0);
  }

  const rotate = useCallback(
    (direction: number) => {
      track(USAGE_EVENT.PREVIEW.IMAGE_ROTATE);
      setRotation(
        (current) =>
          (current + direction * IMAGE_ROTATION_STEP + IMAGE_ROTATION_FULL) %
          IMAGE_ROTATION_FULL,
      );
      setPan(NO_PAN);
    },
    [track],
  );

  const zoomTo = useCallback(
    (value: number) => {
      countZoom();
      const next = clamp(value);
      setZoom(next);
      if (next <= IMAGE_ZOOM_MIN) setPan(NO_PAN);
    },
    [countZoom],
  );

  const stepZoom = useCallback(
    (delta: number) => {
      countZoom();
      setZoom((current) => {
        const next = clamp(current + delta);
        if (next <= IMAGE_ZOOM_MIN) setPan(NO_PAN);
        return next;
      });
    },
    [countZoom],
  );

  const reset = useCallback(() => {
    setZoom(IMAGE_ZOOM_MIN);
    setPan(NO_PAN);
    setRotation(0);
  }, []);

  return { zoom, pan, rotation, rotate, setPan, zoomTo, stepZoom, reset };
};

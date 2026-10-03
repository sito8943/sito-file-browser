import { useEffect, useState, type RefObject } from "react";

// A local path only means the image can be requested. Keep the placeholder until the actual
// displayed element is decoded, including cached images whose load event may already have fired.
export const useImageLoad = (
  imageRef: RefObject<HTMLImageElement | null>,
  src: string,
) => {
  const [result, setResult] = useState<{ src: string; failed: boolean } | null>(
    null,
  );

  useEffect(() => {
    const image = imageRef.current;
    if (!image) return;
    let cancelled = false;

    image.decode().then(
      () => {
        if (!cancelled) setResult({ src, failed: false });
      },
      () => {
        if (!cancelled) setResult({ src, failed: true });
      },
    );

    // Navigation can unmount this element while its decode is still pending.
    return () => {
      cancelled = true;
    };
  }, [imageRef, src]);

  const settled = result?.src === src;
  return {
    loading: !settled,
    failed: settled && result.failed,
    ready: settled && !result.failed,
  };
};

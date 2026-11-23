import type { ReactNode } from "react";

export type DialogProps = {
  visible: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
  // Clicking the backdrop closes the dialog (default true). Turn off for forms whose input would
  // be lost by a stray click outside.
  closeOnBackdropClick?: boolean;
};

import { t } from "@/lang";

import { SAS_LENGTH, SVG_DATA_URL_PREFIX } from "./constants";

type RemoteErrorCode = keyof typeof t.remoteControl.errors;

const isKnownCode = (code: string): code is RemoteErrorCode =>
  code in t.remoteControl.errors;

// Map a REMOTE_* code (or any thrown value) from the backend to localized copy.
export const remoteErrorMessage = (error: unknown): string => {
  const code = String(error);
  return isKnownCode(code)
    ? t.remoteControl.errors[code]
    : t.remoteControl.genericError(code);
};

export const svgDataUrl = (svg: string): string =>
  SVG_DATA_URL_PREFIX + encodeURIComponent(svg);

// Digits typed for the pairing code (spaces and separators dropped), capped at SAS_LENGTH.
export const sasDigits = (value: string): string =>
  value.replace(/\D/g, "").slice(0, SAS_LENGTH);

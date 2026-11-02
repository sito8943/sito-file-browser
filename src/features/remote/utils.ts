import { t } from "@/lang";

import { SVG_DATA_URL_PREFIX } from "./constants";

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

// "123456" → "123 456": two groups read back faster when comparing two screens.
export const formatSas = (sas: string): string =>
  sas.length === 6 ? `${sas.slice(0, 3)} ${sas.slice(3)}` : sas;

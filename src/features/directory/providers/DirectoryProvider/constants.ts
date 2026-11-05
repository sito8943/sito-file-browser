import { FILE_CATEGORY, type FileCategory } from "@/shared/constants";
import { USAGE_EVENT } from "@/shared/usage";

// The usage feature counted when the in-app preview opens a file of each kind. Only the kinds the
// preview renders; any other category opens nothing worth counting as a preview kind.
export const PREVIEW_KIND_USAGE: Partial<Record<FileCategory, string>> = {
  [FILE_CATEGORY.IMAGE]: USAGE_EVENT.PREVIEW.IMAGE,
  [FILE_CATEGORY.VIDEO]: USAGE_EVENT.PREVIEW.VIDEO,
  [FILE_CATEGORY.AUDIO]: USAGE_EVENT.PREVIEW.AUDIO,
  [FILE_CATEGORY.PDF]: USAGE_EVENT.PREVIEW.PDF,
  [FILE_CATEGORY.MARKDOWN]: USAGE_EVENT.PREVIEW.MARKDOWN,
};

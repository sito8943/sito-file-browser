import type { FileTypeExtensions } from "@sito/file-icons";

import type { Tag } from "@/shared/models";

// Inputs for `entryIconColor`: what an entry is, its Finder tags, and the live file-type settings.
export type EntryIconColorArgs = {
  isDir: boolean;
  extension: string;
  tags: readonly Tag[];
  extensions: FileTypeExtensions;
  colorfulFileTypes: boolean;
};

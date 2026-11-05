import { RECENTS, SFTP_SCHEME, TRASH_DIR_NAME } from "@/shared/constants";
import { USAGE_EVENT } from "@/shared/usage";
import { isTagsPath } from "@/shared/utils";

// Which browse feature showing `path` counts: the virtual and remote listings are separate
// features from browsing a real local folder.
export const browseUsageFeature = (path: string): string => {
  if (path === RECENTS) return USAGE_EVENT.BROWSE.RECENTS;
  if (isTagsPath(path)) return USAGE_EVENT.BROWSE.TAGS;
  if (path.endsWith(`/${TRASH_DIR_NAME}`)) return USAGE_EVENT.BROWSE.TRASH;
  if (path.startsWith(SFTP_SCHEME)) return USAGE_EVENT.BROWSE.SFTP;
  return USAGE_EVENT.BROWSE.FOLDER;
};

import type { FileCategory } from "@sito/file-icons";

// The rejection shown under the input when the typed extension already belongs elsewhere: one
// extension may only live in one category, so the add is refused and names the owner.
export type DuplicateError = {
  extension: string;
  category: FileCategory;
};

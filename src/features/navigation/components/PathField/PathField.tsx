import { useState } from "react";

import { USAGE_EVENT, useUsage } from "@/shared/usage";

import PathCrumbs from "../PathCrumbs";
import PathInput from "../PathInput";
import type { PathFieldProps } from "./types";

// The PathBar's central field: breadcrumbs by default, switching to the editable path input when
// the empty area is clicked (for copy/paste). The parent remounts this via `key={path}` on every
// navigation, which resets it back to breadcrumb mode.
const PathField = ({ path, onCommit }: PathFieldProps) => {
  const [editing, setEditing] = useState(false);
  const { track } = useUsage();

  // Both modes commit through the same callback, so the usage split (typed path vs crumb click)
  // happens here, where it is still known which one the user used.
  const commitTyped = (next: string) => {
    track(USAGE_EVENT.NAV.PATH_TYPED);
    onCommit(next);
  };
  const commitCrumb = (next: string) => {
    track(USAGE_EVENT.NAV.CRUMB);
    onCommit(next);
  };

  if (editing)
    return (
      <PathInput
        path={path}
        onCommit={commitTyped}
        autoFocus
        onExit={() => setEditing(false)}
      />
    );

  return (
    <PathCrumbs
      path={path}
      onNavigate={commitCrumb}
      onEditRequest={() => setEditing(true)}
    />
  );
};

export default PathField;

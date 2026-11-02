import { createContext, useContext } from "react";

import type { RemoteContextValue } from "./types";

export const RemoteContext = createContext<RemoteContextValue | null>(null);

export const useRemote = () => {
  const value = useContext(RemoteContext);
  if (!value) throw new Error("useRemote must be used within RemoteProvider");
  return value;
};

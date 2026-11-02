import type { REMOTE_STATE } from "./constants";

export type RemoteStateId = (typeof REMOTE_STATE)[keyof typeof REMOTE_STATE];

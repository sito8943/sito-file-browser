import { USAGE_PREFIX, usageFeature } from "@/shared/usage";

import { ROUTES, type RouteKey } from "../../routes";

// The page feature of a pathname (`/directory` → `page.directory`), or null for unknown paths.
export const pageUsageFeatureOf = (pathname: string): string | null => {
  const key = (Object.keys(ROUTES) as RouteKey[]).find(
    (routeKey) => ROUTES[routeKey] === pathname,
  );
  return key ? usageFeature(USAGE_PREFIX.PAGE, key) : null;
};

import { usageFeature, type UsagePrefix } from "@/shared/usage";

// Catalogue entries for every id of a const-object registry (ENTRY_ACTION, SIDEBAR_ACTION, …).
export const registryUsageFeatures = (
  prefix: UsagePrefix,
  registry: Record<string, string>,
): string[] => Object.values(registry).map((id) => usageFeature(prefix, id));

import providerCatalog from "../../../config/providers.json";

export type Protocol = "openai" | "anthropic";
export type Policy = "strict" | "balanced" | "monitor";
export type ProviderId =
  | "custom"
  | "openai"
  | "anthropic"
  | "deepseek"
  | "gemini"
  | "openrouter"
  | "groq"
  | "xai"
  | "mistral"
  | "kimi"
  | "dashscope"
  | "siliconflow";
export type ProviderPreset = {
  id: ProviderId;
  name: string;
  urls: Partial<Record<Protocol, string>>;
};
export type GatewayStatus = { running: boolean };
export type StartupIssue = {
  profile_id: string;
  profile_name: string;
  reason: "missing_credentials" | "invalid_mapping_key" | "invalid_dictionary";
};
export type ProfileSummary = {
  id: string;
  name: string;
  protocol: Protocol;
  upstream_url: string;
  policy: Policy;
  custom_term_count: number;
};
export type EditableProfile = {
  id: string;
  name: string;
  provider: ProviderId;
  protocol: Protocol;
  upstream_url: string;
  policy: Policy;
  custom_terms: string[];
};
export type ProfileDraft = {
  name: string;
  protocol: Protocol;
  upstreamUrl: string;
  upstreamKey: string;
  discoveryId?: string;
  policy: Policy;
  customTerms: string[];
};
export type DiscoveredProfile = {
  id: string;
  provider: ProviderId;
  name: string;
  protocol: Protocol;
  upstream_url: string;
  key_preview: string;
  source: string;
  key_variable: string;
  url_variable?: string;
};
export type UsageTotals = {
  profile_id: string;
  request_count: number;
  metered_request_count: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  cached_tokens: number;
};
export type UsageResponse = {
  profiles: UsageTotals[];
  retention_hours: number;
};
export type BalanceSummary = {
  supported: boolean;
  provider?: string;
  metric?: "account_balance" | "api_key_limit";
  remaining?: number;
  total?: number;
  used?: number;
  unit?: string;
  detail?: string;
  refreshed_at_ms?: number;
};
export type ConnectionTestSummary = {
  success: boolean;
  status: number;
  latency_ms: number;
};
export type CreatedProfile = {
  profile: ProfileSummary;
  local_base_url: string;
  local_key: string;
};
export type RotatedLocalKey = { profile_id: string; local_key: string };
export type ActivityPreview = { kind: string; display: string };
export type PrivacyActivityItem = {
  id: string;
  occurred_at_ms: number;
  profile_id: string;
  endpoint: string;
  model: string;
  status: number;
  latency_ms: number;
  category_counts: Record<string, number>;
  actions: string[];
  request_id: string;
  previews: ActivityPreview[];
  preview_available: boolean;
};
export type PrivacyActivityResponse = {
  activities: PrivacyActivityItem[];
  total_activity_count: number;
  session_preview_count: number;
  persist_dropped_count: number;
  retention_hours: number;
};

export function defaultUpstreamUrl(protocol: Protocol): string {
  return protocol === "anthropic"
    ? "https://api.anthropic.com"
    : "https://api.openai.com/v1";
}

export const providerPresets: ProviderPreset[] = providerCatalog.map((provider) => ({
  id: provider.id as ProviderId,
  name: provider.name,
  urls: provider.urls as Partial<Record<Protocol, string>>,
}));

export function providerPreset(id: ProviderId): ProviderPreset {
  return providerPresets.find((provider) => provider.id === id) ?? providerPresets[0];
}

export function providerProtocols(id: ProviderId): Protocol[] {
  if (id === "custom") return ["openai", "anthropic"];
  const urls = providerPreset(id).urls;
  return (["openai", "anthropic"] as Protocol[]).filter((protocol) => Boolean(urls[protocol]));
}

export function providerDefaultUrl(id: ProviderId, protocol: Protocol): string {
  return providerPreset(id).urls[protocol] ?? defaultUpstreamUrl(protocol);
}

const balanceHosts = new Set([
  "api.deepseek.com",
  "api.moonshot.cn",
  "api.siliconflow.cn",
  "openrouter.ai",
]);

export function supportsBalanceQuery(upstreamUrl: string): boolean {
  try {
    return balanceHosts.has(new URL(upstreamUrl).hostname);
  } catch {
    return false;
  }
}

export function localBaseUrl(protocol: Protocol): string {
  return protocol === "anthropic"
    ? "http://127.0.0.1:17891"
    : "http://127.0.0.1:17891/v1";
}

export function sameUpstreamOrigin(left: string, right: string): boolean {
  try {
    return new URL(left).origin === new URL(right).origin;
  } catch {
    return false;
  }
}

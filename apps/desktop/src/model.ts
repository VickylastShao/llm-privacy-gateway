export type Protocol = "openai" | "anthropic";
export type Policy = "strict" | "balanced" | "monitor";
export type GatewayStatus = { running: boolean };
export type ProfileSummary = {
  id: string;
  name: string;
  protocol: Protocol;
  upstream_url: string;
  policy: Policy;
};
export type ProfileDraft = {
  name: string;
  protocol: Protocol;
  upstreamUrl: string;
  upstreamKey: string;
  policy: Policy;
};
export type CreatedProfile = {
  profile: ProfileSummary;
  local_base_url: string;
  local_key: string;
};
export type RotatedLocalKey = { profile_id: string; local_key: string };

export function defaultUpstreamUrl(protocol: Protocol): string {
  return protocol === "anthropic"
    ? "https://api.anthropic.com"
    : "https://api.openai.com/v1";
}

export function localBaseUrl(protocol: Protocol): string {
  return protocol === "anthropic"
    ? "http://127.0.0.1:17891"
    : "http://127.0.0.1:17891/v1";
}

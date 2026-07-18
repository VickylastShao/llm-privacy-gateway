export type GatewayStatus = { running: boolean };
export type ProfileSummary = {
  id: string;
  name: string;
  protocol: string;
  upstream_url: string;
  policy: string;
};
export type ProfileDraft = {
  name: string;
  protocol: string;
  upstreamUrl: string;
  upstreamKey: string;
  policy: string;
};
export type CreatedProfile = {
  profile: ProfileSummary;
  local_base_url: string;
  local_key: string;
};
export type RotatedLocalKey = { profile_id: string; local_key: string };

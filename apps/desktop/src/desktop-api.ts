import { invoke } from "@tauri-apps/api/core";
import type {
  CreatedProfile,
  GatewayStatus,
  ProfileDraft,
  ProfileSummary,
  PrivacyActivityResponse,
  RotatedLocalKey,
} from "./model";

export interface DesktopApi {
  gatewayStatus(): Promise<GatewayStatus>;
  startGateway(): Promise<GatewayStatus>;
  stopGateway(): Promise<GatewayStatus>;
  listProfiles(): Promise<ProfileSummary[]>;
  createProfile(draft: ProfileDraft): Promise<CreatedProfile>;
  rotateLocalKey(id: string): Promise<RotatedLocalKey>;
  deleteProfile(id: string): Promise<void>;
  queryActivity(limit: number): Promise<PrivacyActivityResponse>;
}

export const desktopApi: DesktopApi = {
  gatewayStatus: () => invoke("gateway_status"),
  startGateway: () => invoke("start_gateway"),
  stopGateway: () => invoke("stop_gateway"),
  listProfiles: () => invoke("list_profiles"),
  createProfile: (draft) => invoke("create_profile", { draft }),
  rotateLocalKey: (id) => invoke("rotate_local_key", { id }),
  deleteProfile: (id) => invoke("delete_profile", { id }),
  queryActivity: (limit) => invoke("query_activity", { limit }),
};

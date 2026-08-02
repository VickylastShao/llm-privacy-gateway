import { invoke } from "@tauri-apps/api/core";
import type {
  CreatedProfile,
  BalanceSummary,
  ConnectionTestSummary,
  DiscoveredProfile,
  EditableProfile,
  GatewayStatus,
  ProfileDraft,
  ProfileSummary,
  PrivacyActivityResponse,
  RotatedLocalKey,
  StartupIssue,
  UsageResponse,
} from "./model";

export interface DesktopApi {
  gatewayStatus(): Promise<GatewayStatus>;
  startGateway(): Promise<GatewayStatus>;
  stopGateway(): Promise<GatewayStatus>;
  listProfiles(): Promise<ProfileSummary[]>;
  listStartupIssues(): Promise<StartupIssue[]>;
  removeUnavailableProfile(id: string): Promise<void>;
  getProfileForEdit(id: string): Promise<EditableProfile>;
  createProfile(draft: ProfileDraft): Promise<CreatedProfile>;
  updateProfile(id: string, draft: ProfileDraft): Promise<ProfileSummary>;
  rotateLocalKey(id: string): Promise<RotatedLocalKey>;
  deleteProfile(id: string): Promise<void>;
  queryActivity(limit: number): Promise<PrivacyActivityResponse>;
  scanEnvironmentProfiles(): Promise<DiscoveredProfile[]>;
  clearEnvironmentDiscoveries(): Promise<void>;
  queryUsage(): Promise<UsageResponse>;
  queryBalance(id: string): Promise<BalanceSummary>;
  testProfileConnection(id: string): Promise<ConnectionTestSummary>;
}

export const desktopApi: DesktopApi = {
  gatewayStatus: () => invoke("gateway_status"),
  startGateway: () => invoke("start_gateway"),
  stopGateway: () => invoke("stop_gateway"),
  listProfiles: () => invoke("list_profiles"),
  listStartupIssues: () => invoke("list_startup_issues"),
  removeUnavailableProfile: (id) => invoke("remove_unavailable_profile", { id }),
  getProfileForEdit: (id) => invoke("get_profile_for_edit", { id }),
  createProfile: (draft) => invoke("create_profile", { draft }),
  updateProfile: (id, draft) => invoke("update_profile", { id, draft }),
  rotateLocalKey: (id) => invoke("rotate_local_key", { id }),
  deleteProfile: (id) => invoke("delete_profile", { id }),
  queryActivity: (limit) => invoke("query_activity", { limit }),
  scanEnvironmentProfiles: () => invoke("scan_environment_profiles"),
  clearEnvironmentDiscoveries: () => invoke("clear_environment_discoveries"),
  queryUsage: () => invoke("query_usage"),
  queryBalance: (id) => invoke("query_balance", { id }),
  testProfileConnection: (id) => invoke("test_profile_connection", { id }),
};

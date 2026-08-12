import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { DesktopApi } from "./desktop-api";
import type {
  GatewayStatus,
  PrivacyActivityResponse,
  ProfileSummary,
} from "./model";

const profile: ProfileSummary = {
  id: "profile-1",
  name: "DeepSeek",
  protocol: "openai",
  upstream_url: "https://api.deepseek.com",
  policy: "strict",
  custom_term_count: 0,
};

const emptyActivity: PrivacyActivityResponse = {
  activities: [],
  total_activity_count: 0,
  session_preview_count: 0,
  persist_dropped_count: 0,
  retention_hours: 168,
};

const interceptedActivity: PrivacyActivityResponse = {
  activities: [
    {
      id: "activity-1",
      occurred_at_ms: Date.UTC(2026, 6, 18, 2, 30),
      profile_id: profile.id,
      endpoint: "/chat/completions",
      model: "deepseek-chat",
      status: 403,
      latency_ms: 12,
      category_counts: { api_key: 1, custom: 2 },
      actions: ["block", "pseudonymize"],
      request_id: "request-1",
      previews: [{ kind: "api_key", display: "sk-•••••••xyz" }],
      preview_available: true,
    },
  ],
  total_activity_count: 1,
  session_preview_count: 1,
  persist_dropped_count: 0,
  retention_hours: 168,
};

function emptyApi(
  profiles: ProfileSummary[] = [],
  status: GatewayStatus = { running: false },
): DesktopApi {
  return {
    gatewayStatus: vi.fn().mockResolvedValue(status),
    startGateway: vi.fn().mockResolvedValue({ running: true }),
    stopGateway: vi.fn().mockResolvedValue({ running: false }),
    listProfiles: vi.fn().mockResolvedValue(profiles),
    listStartupIssues: vi.fn().mockResolvedValue([]),
    removeUnavailableProfile: vi.fn(),
    getProfileForEdit: vi.fn(),
    createProfile: vi.fn(),
    updateProfile: vi.fn(),
    rotateLocalKey: vi.fn(),
    deleteProfile: vi.fn(),
    queryActivity: vi.fn().mockResolvedValue(emptyActivity),
    scanEnvironmentProfiles: vi.fn().mockResolvedValue([]),
    clearEnvironmentDiscoveries: vi.fn().mockResolvedValue(undefined),
    queryUsage: vi.fn().mockResolvedValue({ profiles: [], retention_hours: 168 }),
    queryBalance: vi.fn().mockResolvedValue({ supported: false }),
    testProfileConnection: vi.fn(),
  };
}

describe("desktop single-page shell", () => {
  it("shows one guided first-run workflow without legacy navigation", async () => {
    render(<App api={emptyApi()} initialLanguage="en-US" />);

    await waitFor(() => expect(screen.getByText("Add model connection")).toBeTruthy());
    expect(screen.getByText("Copy local settings")).toBeTruthy();
    expect(screen.getAllByText("Start protection")).toHaveLength(2);
    expect(screen.getAllByRole("form")).toHaveLength(1);
    expect(
      screen.getByRole("textbox", { name: "Connection name" }).hasAttribute("autofocus"),
    ).toBe(false);
    expect(screen.queryByText("Dashboard")).toBeNull();
    expect(screen.queryByText("Policies")).toBeNull();
    expect(screen.queryByText("Test Console")).toBeNull();
    expect(screen.queryByText("Activity")).toBeNull();
  });

  it("surfaces and explicitly removes an unavailable saved connection", async () => {
    const user = userEvent.setup();
    const api = emptyApi();
    vi.mocked(api.listStartupIssues).mockResolvedValue([
      {
        profile_id: "broken-profile",
        profile_name: "Old DeepSeek",
        reason: "missing_credentials",
      },
    ]);
    vi.mocked(api.removeUnavailableProfile).mockResolvedValue(undefined);
    render(<App api={api} initialLanguage="en-US" />);

    expect(await screen.findByText("A saved connection needs attention")).toBeTruthy();
    expect(screen.getByText("Old DeepSeek")).toBeTruthy();
    expect(
      screen.getByText("One or more credentials are missing from the operating system vault."),
    ).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Remove unavailable connection" }));
    const dialog = screen.getByRole("dialog", { name: "Remove Old DeepSeek?" });
    await user.click(
      within(dialog).getByRole("button", { name: "Remove unavailable connection" }),
    );

    await waitFor(() =>
      expect(api.removeUnavailableProfile).toHaveBeenCalledWith("broken-profile"),
    );
    expect(screen.queryByText("A saved connection needs attention")).toBeNull();
  });

  it("switches all visible workflow copy from Chinese to English", async () => {
    const user = userEvent.setup();
    render(<App api={emptyApi()} initialLanguage="zh-CN" />);

    await waitFor(() => expect(screen.getByText("添加模型连接")).toBeTruthy());
    expect(screen.getByText("连接第一个模型")).toBeTruthy();
    expect(screen.getByRole("form", { name: "模型连接" })).toBeTruthy();
    expect(screen.getAllByText("启动保护").length).toBeGreaterThan(0);

    await user.click(screen.getByRole("button", { name: "English" }));

    expect(screen.getByText("Connect your first model")).toBeTruthy();
    expect(screen.getByText("Add model connection")).toBeTruthy();
    expect(screen.getByRole("form", { name: "Model connection" })).toBeTruthy();
    expect(screen.getAllByText("Start protection").length).toBeGreaterThan(0);
  });

  it("changes only an untouched protocol default URL", async () => {
    const user = userEvent.setup();
    render(<App api={emptyApi()} initialLanguage="en-US" />);

    const url = await screen.findByRole("textbox", { name: "Upstream Base URL" });
    expect((url as HTMLInputElement).value).toBe("https://api.openai.com/v1");

    await user.selectOptions(screen.getByRole("combobox", { name: "Provider" }), "custom");

    await user.click(screen.getByRole("radio", { name: "Anthropic Messages" }));
    expect((url as HTMLInputElement).value).toBe("https://api.anthropic.com");

    await user.clear(url);
    await user.type(url, "https://api.example.test/custom");
    await user.click(screen.getByRole("radio", { name: "OpenAI-compatible" }));
    expect((url as HTMLInputElement).value).toBe("https://api.example.test/custom");

    await user.click(screen.getByRole("button", { name: "Reset to protocol default" }));
    expect((url as HTMLInputElement).value).toBe("https://api.openai.com/v1");
  });

  it("creates a strict profile and focuses the one-time local settings", async () => {
    const user = userEvent.setup();
    const api = emptyApi();
    vi.mocked(api.createProfile).mockResolvedValue({
      profile: {
        id: "profile-1",
        name: "DeepSeek",
        protocol: "openai",
        upstream_url: "https://api.deepseek.com",
        policy: "strict",
        custom_term_count: 0,
      },
      local_base_url: "http://127.0.0.1:17891/v1",
      local_key: "lpg_local_once",
    });
    render(<App api={api} initialLanguage="en-US" />);

    await user.type(await screen.findByRole("textbox", { name: "Connection name" }), "DeepSeek");
    const url = screen.getByRole("textbox", { name: "Upstream Base URL" });
    await user.clear(url);
    await user.type(url, "https://api.deepseek.com");
    await user.type(screen.getByLabelText("Upstream API key"), "test-upstream-key");

    const advanced = screen.getByText("Advanced privacy settings").closest("details");
    expect(advanced?.open).toBe(false);
    await user.click(screen.getByRole("button", { name: "Create connection" }));

    await waitFor(() =>
      expect(api.createProfile).toHaveBeenCalledWith({
        name: "DeepSeek",
        protocol: "openai",
        upstreamUrl: "https://api.deepseek.com",
        upstreamKey: "test-upstream-key",
        discoveryId: undefined,
        policy: "strict",
        customTerms: [],
      }),
    );
    expect(await screen.findByText("lpg_local_once")).toBeTruthy();
    const result = screen.getByRole("region", { name: "Copy these local settings now" });
    expect(document.activeElement).toBe(result);
  });

  it("submits normalized per-connection custom sensitive terms", async () => {
    const user = userEvent.setup();
    const api = emptyApi();
    vi.mocked(api.createProfile).mockResolvedValue({
      profile: { ...profile, custom_term_count: 2 },
      local_base_url: "http://127.0.0.1:17891/v1",
      local_key: "lpg_local_once",
    });
    render(<App api={api} initialLanguage="en-US" />);

    await user.type(await screen.findByRole("textbox", { name: "Connection name" }), "Private");
    await user.type(screen.getByLabelText("Upstream API key"), "test-upstream-key");
    await user.click(screen.getByText("Advanced privacy settings"));
    await user.type(
      screen.getByRole("textbox", { name: "Custom sensitive terms" }),
      " Project Aurora {enter}Internal Client ",
    );
    await user.click(screen.getByRole("button", { name: "Create connection" }));

    await waitFor(() =>
      expect(api.createProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          customTerms: ["Project Aurora", "Internal Client"],
        }),
      ),
    );
  });

  it("discovers a saved environment configuration and imports it without exposing the key", async () => {
    const user = userEvent.setup();
    const api = emptyApi();
    vi.mocked(api.scanEnvironmentProfiles).mockResolvedValue([
      {
        id: "discovery-1",
        provider: "deepseek",
        name: "DeepSeek",
        protocol: "openai",
        upstream_url: "https://api.deepseek.com",
        key_preview: "sk-••••••7890",
        source: "user",
        key_variable: "DEEPSEEK_API_KEY",
      },
    ]);
    vi.mocked(api.createProfile).mockResolvedValue({
      profile,
      local_base_url: "http://127.0.0.1:17891/v1",
      local_key: "lpg_local_once",
    });
    render(<App api={api} initialLanguage="en-US" />);

    await waitFor(() => expect(api.scanEnvironmentProfiles).toHaveBeenCalledTimes(1));
    await user.click(await screen.findByRole("button", { name: /DeepSeek/ }));
    expect(screen.queryByText(/123456/)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Create connection" }));

    await waitFor(() =>
      expect(api.createProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          discoveryId: "discovery-1",
          upstreamKey: "",
          upstreamUrl: "https://api.deepseek.com",
        }),
      ),
    );
  });

  it("clears temporary environment discoveries when the add form is cancelled", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.scanEnvironmentProfiles).mockResolvedValue([
      {
        id: "discovery-1",
        provider: "deepseek",
        name: "DeepSeek",
        protocol: "openai",
        upstream_url: "https://api.deepseek.com",
        key_preview: "sk-••••••7890",
        source: "user",
        key_variable: "DEEPSEEK_API_KEY",
      },
    ]);
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "Add connection" }));
    const form = await screen.findByRole("form", { name: "Model connection" });
    expect(await within(form).findByRole("button", { name: /User environment/ })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(api.clearEnvironmentDiscoveries).toHaveBeenCalledOnce());
    expect(screen.queryByRole("form", { name: "Model connection" })).toBeNull();
  });

  it("shows provider-reported token totals and queries a verified balance endpoint", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.queryUsage).mockResolvedValue({
      profiles: [
        {
          profile_id: profile.id,
          request_count: 3,
          metered_request_count: 2,
          input_tokens: 1_200,
          output_tokens: 34,
          total_tokens: 1_234,
          cached_tokens: 500,
        },
      ],
      retention_hours: 168,
    });
    vi.mocked(api.queryBalance).mockResolvedValue({
      supported: true,
      provider: "DeepSeek",
      metric: "account_balance",
      remaining: 42.5,
      unit: "CNY",
    });
    render(<App api={api} initialLanguage="en-US" />);

    expect(await screen.findByText("1,234")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Check quota / balance" }));
    expect(await screen.findByText("Account balance: 42.5 CNY")).toBeTruthy();
    expect(api.queryBalance).toHaveBeenCalledWith(profile.id);
  });

  it("shows a persistent connection card without revealing either key", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    render(<App api={api} initialLanguage="en-US" />);

    expect(await screen.findByText("DeepSeek")).toBeTruthy();
    expect(screen.getByText("https://api.deepseek.com")).toBeTruthy();
    expect(screen.getByText("http://127.0.0.1:17891/v1")).toBeTruthy();
    expect(screen.getByText("Strict")).toBeTruthy();
    expect(screen.queryByText(/lpg_local_/)).toBeNull();
    expect(screen.queryByText(/test-upstream-key/)).toBeNull();

    await user.click(screen.getByRole("button", { name: "Copy local Base URL" }));
    expect(await navigator.clipboard.readText()).toBe("http://127.0.0.1:17891/v1");
  });

  it("reports a clipboard failure instead of claiming the Base URL was copied", async () => {
    const user = userEvent.setup();
    vi.spyOn(navigator.clipboard, "writeText").mockRejectedValueOnce(new Error("denied"));
    render(<App api={emptyApi([profile])} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "Copy local Base URL" }));

    expect((await screen.findByRole("alert")).textContent).toBe(
      "Could not copy. Select the value and copy it manually.",
    );
    expect(screen.queryByRole("button", { name: "Copied" })).toBeNull();
  });

  it("shows how many custom sensitive terms are configured without exposing them on the card", async () => {
    const api = emptyApi([{ ...profile, custom_term_count: 2 }]);
    render(<App api={api} initialLanguage="en-US" />);

    expect(await screen.findByText("Custom sensitive terms: 2")).toBeTruthy();
    expect(screen.queryByText("Project Aurora")).toBeNull();
  });

  it("tests an upstream connection and reports authenticated latency", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.testProfileConnection).mockResolvedValue({
      success: true,
      status: 200,
      latency_ms: 87,
    });
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "Test connection" }));

    expect(await screen.findByText("Connected · 87 ms")).toBeTruthy();
    expect(api.testProfileConnection).toHaveBeenCalledWith(profile.id);
  });

  it("uses an accessible dismissible overflow menu with only low-frequency actions", async () => {
    const user = userEvent.setup();
    render(<App api={emptyApi([profile])} initialLanguage="en-US" />);
    const trigger = await screen.findByRole("button", { name: "More actions for DeepSeek" });

    await user.click(trigger);
    const edit = screen.getByRole("menuitem", { name: "Edit connection" });
    expect(edit).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Rotate local key" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Delete connection" })).toBeTruthy();
    expect(screen.getAllByRole("menuitem")).toHaveLength(3);
    expect(screen.queryByRole("combobox", { name: "Provider" })).toBeNull();
    expect(screen.queryByRole("navigation")).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(edit));
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(
      screen.getByRole("menuitem", { name: "Rotate local key" }),
    );
    await user.keyboard("{End}");
    expect(document.activeElement).toBe(
      screen.getByRole("menuitem", { name: "Delete connection" }),
    );
    await user.keyboard("{Home}");
    expect(document.activeElement).toBe(edit);

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);

    await user.click(trigger);
    await user.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("edits a connection without requiring or rotating its saved keys", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.getProfileForEdit).mockResolvedValue({
      ...profile,
      provider: "deepseek",
      custom_terms: ["Project Aurora"],
    });
    vi.mocked(api.updateProfile).mockResolvedValue({
      ...profile,
      name: "DeepSeek production",
      policy: "balanced",
      custom_term_count: 1,
    });
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "More actions for DeepSeek" }));
    await user.click(screen.getByRole("menuitem", { name: "Edit connection" }));

    expect(
      await screen.findByRole("textbox", { name: "Connection name" }),
    ).toHaveProperty("value", "DeepSeek");
    expect(screen.getByDisplayValue("https://api.deepseek.com")).toBeTruthy();
    expect(screen.getByDisplayValue("Project Aurora")).toBeTruthy();
    expect(screen.getByLabelText("Upstream API key").hasAttribute("required")).toBe(false);

    const name = screen.getByRole("textbox", { name: "Connection name" });
    await user.clear(name);
    await user.type(name, "DeepSeek production");
    await user.click(screen.getByText("Advanced privacy settings"));
    await user.click(screen.getByRole("radio", { name: /Balanced/ }));
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() =>
      expect(api.updateProfile).toHaveBeenCalledWith(profile.id, {
        name: "DeepSeek production",
        protocol: "openai",
        upstreamUrl: "https://api.deepseek.com",
        upstreamKey: "",
        discoveryId: undefined,
        policy: "balanced",
        customTerms: ["Project Aurora"],
      }),
    );
    expect(await screen.findByRole("heading", { name: "DeepSeek production" })).toBeTruthy();
    expect(api.rotateLocalKey).not.toHaveBeenCalled();
  });

  it("requires a replacement key before an edited connection can target another origin", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.getProfileForEdit).mockResolvedValue({
      ...profile,
      provider: "deepseek",
      custom_terms: [],
    });
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "More actions for DeepSeek" }));
    await user.click(screen.getByRole("menuitem", { name: "Edit connection" }));
    const url = await screen.findByRole("textbox", { name: "Upstream Base URL" });
    await user.clear(url);
    await user.type(url, "https://other.example.test/v1");

    expect(screen.getByLabelText("Upstream API key").hasAttribute("required")).toBe(true);
    expect(
      screen.getByText(
        "The upstream origin changed. Enter a new key so the saved key is never sent to another server.",
      ),
    ).toBeTruthy();
  });

  it("rotates a local key and exposes the replacement only once", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.rotateLocalKey).mockResolvedValue({
      profile_id: profile.id,
      local_key: "lpg_local_rotated",
    });
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "More actions for DeepSeek" }));
    await user.click(screen.getByRole("menuitem", { name: "Rotate local key" }));
    expect(screen.getByRole("dialog", { name: "Rotate the local key?" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Rotate key" }));

    await waitFor(() => expect(api.rotateLocalKey).toHaveBeenCalledWith(profile.id));
    expect(await screen.findByText("lpg_local_rotated")).toBeTruthy();
  });

  it("deletes a connection only after confirmation", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.deleteProfile).mockResolvedValue(undefined);
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "More actions for DeepSeek" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete connection" }));
    expect(screen.getByRole("dialog", { name: "Delete DeepSeek?" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Delete connection" }));

    await waitFor(() => expect(api.deleteProfile).toHaveBeenCalledWith(profile.id));
    expect(screen.queryByRole("heading", { name: "DeepSeek" })).toBeNull();
  });

  it("requires protection to stop before a destructive action", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile], { running: true });
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "More actions for DeepSeek" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete connection" }));

    expect(screen.getByText("Stop protection before changing connections.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Delete connection" })).toHaveProperty(
      "disabled",
      true,
    );
    expect(api.deleteProfile).not.toHaveBeenCalled();
  });

  it("starts and stops protection from the single primary control", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "Start protection" }));
    await waitFor(() => expect(api.startGateway).toHaveBeenCalledOnce());
    expect(screen.getByText("Protection on")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Stop protection" }));
    await waitFor(() => expect(api.stopGateway).toHaveBeenCalledOnce());
    expect(screen.getByText("Protection off")).toBeTruthy();
  });

  it("uses modal confirmation semantics and accessible icon controls", async () => {
    const user = userEvent.setup();
    render(<App api={emptyApi([profile])} initialLanguage="en-US" />);

    expect(await screen.findByRole("button", { name: "More actions for DeepSeek" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add connection" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "More actions for DeepSeek" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete connection" }));
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(
      within(dialog).getByRole("button", { name: "Cancel" }),
    );
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders an unknown runtime error as text rather than markup", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.startGateway).mockRejectedValue("<img src=x onerror=alert(1)>");
    const { container } = render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "Start protection" }));
    expect(await screen.findByText("<img src=x onerror=alert(1)>")).toBeTruthy();
    expect(container.querySelector("img")).toBeNull();
  });

  it("shows intercepted privacy categories and only safe masked previews", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.queryActivity).mockResolvedValue(interceptedActivity);
    render(<App api={api} initialLanguage="en-US" />);

    expect(await screen.findByText("Privacy activity")).toBeTruthy();
    expect(screen.getByText("API key ×1")).toBeTruthy();
    expect(screen.getByText("Custom term ×2")).toBeTruthy();
    expect(screen.getAllByText("Blocked")).toHaveLength(2);
    expect(screen.queryByText("sk-secret-original-xyz")).toBeNull();

    await user.click(screen.getByText("deepseek-chat"));
    expect(screen.getByText("sk-•••••••xyz")).toBeTruthy();
    expect(screen.getByText("Available only in this app session")).toBeTruthy();
    expect(screen.getByText("Pseudonymized")).toBeTruthy();
  });

  it("explains when balanced mode forwards content that was not inspected", async () => {
    const user = userEvent.setup();
    const api = emptyApi([{ ...profile, policy: "balanced" }]);
    vi.mocked(api.queryActivity).mockResolvedValue({
      ...interceptedActivity,
      activities: [
        {
          ...interceptedActivity.activities[0],
          status: 200,
          category_counts: { uninspected_content: 1 },
          actions: ["warn"],
          previews: [{ kind: "uninspected_content", display: "Image content" }],
        },
      ],
    });
    render(<App api={api} initialLanguage="en-US" />);

    expect(await screen.findByText("Uninspected content ×1")).toBeTruthy();
    expect(screen.getAllByText("Partially inspected")).toHaveLength(2);

    await user.click(screen.getByText("deepseek-chat"));
    expect(screen.getByText("Uninspected content types")).toBeTruthy();
    expect(screen.getByText("Image content")).toBeTruthy();
  });

  it("polls while protection runs without closing an expanded activity", async () => {
    vi.useFakeTimers();
    try {
      const api = emptyApi([profile], { running: true });
      vi.mocked(api.queryActivity).mockResolvedValue(interceptedActivity);
      render(<App api={api} initialLanguage="en-US" />);

      await act(async () => Promise.resolve());
      const details = screen.getByText("deepseek-chat").closest("details");
      if (!details) throw new Error("activity details missing");
      fireEvent.click(details.querySelector("summary") as HTMLElement);
      expect(details.open).toBe(true);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_000);
      });
      expect(api.queryActivity).toHaveBeenCalledTimes(2);
      expect(details.open).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("loads the full bounded history after one show-more action", async () => {
    const user = userEvent.setup();
    const api = emptyApi([profile]);
    vi.mocked(api.queryActivity).mockResolvedValue({
      ...interceptedActivity,
      total_activity_count: 20,
    });
    render(<App api={api} initialLanguage="en-US" />);

    await user.click(await screen.findByRole("button", { name: "Show more" }));
    await waitFor(() => expect(api.queryActivity).toHaveBeenCalledWith(100));
  });
});

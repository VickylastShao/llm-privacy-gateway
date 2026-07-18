import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { DesktopApi } from "./desktop-api";
import type { GatewayStatus, ProfileSummary } from "./model";

const profile: ProfileSummary = {
  id: "profile-1",
  name: "DeepSeek",
  protocol: "openai",
  upstream_url: "https://api.deepseek.com",
  policy: "strict",
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
    createProfile: vi.fn(),
    rotateLocalKey: vi.fn(),
    deleteProfile: vi.fn(),
  };
}

describe("desktop single-page shell", () => {
  it("shows one guided first-run workflow without legacy navigation", async () => {
    render(<App api={emptyApi()} initialLanguage="en-US" />);

    await waitFor(() => expect(screen.getByText("Add model connection")).toBeTruthy());
    expect(screen.getByText("Copy local settings")).toBeTruthy();
    expect(screen.getAllByText("Start protection")).toHaveLength(2);
    expect(screen.getAllByRole("form")).toHaveLength(1);
    expect(screen.queryByText("Dashboard")).toBeNull();
    expect(screen.queryByText("Policies")).toBeNull();
    expect(screen.queryByText("Test Console")).toBeNull();
    expect(screen.queryByText("Activity")).toBeNull();
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
        policy: "strict",
      }),
    );
    expect(await screen.findByText("lpg_local_once")).toBeTruthy();
    const result = screen.getByRole("region", { name: "Copy these local settings now" });
    expect(document.activeElement).toBe(result);
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

  it("uses an accessible dismissible overflow menu with only low-frequency actions", async () => {
    const user = userEvent.setup();
    render(<App api={emptyApi([profile])} initialLanguage="en-US" />);
    const trigger = await screen.findByRole("button", { name: "More actions for DeepSeek" });

    await user.click(trigger);
    expect(screen.getByRole("menuitem", { name: "Rotate local key" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Delete connection" })).toBeTruthy();
    expect(screen.getAllByRole("menuitem")).toHaveLength(2);
    expect(document.querySelector("select")).toBeNull();
    expect(screen.queryByRole("navigation")).toBeNull();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);

    await user.click(trigger);
    await user.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
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
    expect(screen.queryByText("DeepSeek")).toBeNull();
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
});

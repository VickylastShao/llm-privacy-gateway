import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { DesktopApi } from "./desktop-api";

function emptyApi(): DesktopApi {
  return {
    gatewayStatus: vi.fn().mockResolvedValue({ running: false }),
    startGateway: vi.fn().mockResolvedValue({ running: true }),
    stopGateway: vi.fn().mockResolvedValue({ running: false }),
    listProfiles: vi.fn().mockResolvedValue([]),
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
});

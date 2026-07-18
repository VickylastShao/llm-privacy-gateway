import { render, screen, waitFor } from "@testing-library/react";
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
});

import { describe, expect, it } from "vitest";
import {
  detectLocale,
  initialLocale,
  localizeError,
  setLocalePreference,
} from "./i18n";

describe("desktop locale selection", () => {
  it("uses Chinese only for Chinese browser locales", () => {
    expect(detectLocale("zh-CN")).toBe("zh-CN");
    expect(detectLocale("zh-Hans")).toBe("zh-CN");
    expect(detectLocale("en-US")).toBe("en");
    expect(detectLocale("fr-FR")).toBe("en");
  });

  it("prefers and persists the user's explicit locale", () => {
    localStorage.setItem("lpg.locale", "en");
    expect(initialLocale("zh-CN")).toBe("en");

    setLocalePreference("zh-CN");
    expect(localStorage.getItem("lpg.locale")).toBe("zh-CN");
  });
});

describe("desktop error localization", () => {
  it("maps known backend failures to actionable copy", () => {
    expect(
      localizeError("zh-CN", "create a profile before starting the gateway"),
    ).toEqual({ summary: "请先添加连接，再启动保护。" });
  });

  it("keeps an unknown failure as secondary plain-text detail", () => {
    expect(localizeError("en", "<img src=x onerror=alert(1)>")).toEqual({
      summary: "The operation failed.",
      detail: "<img src=x onerror=alert(1)>",
    });
  });
});

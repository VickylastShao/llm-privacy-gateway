import { describe, expect, it } from "vitest";
import {
  defaultUpstreamUrl,
  localBaseUrl,
  providerDefaultUrl,
  providerProtocols,
  sameUpstreamOrigin,
  supportsBalanceQuery,
} from "./model";

describe("protocol URLs", () => {
  it("derives the protocol-specific local address", () => {
    expect(localBaseUrl("openai")).toBe("http://127.0.0.1:17891/v1");
    expect(localBaseUrl("anthropic")).toBe("http://127.0.0.1:17891");
  });

  it("provides editable official provider presets", () => {
    expect(providerDefaultUrl("deepseek", "openai")).toBe("https://api.deepseek.com");
    expect(providerDefaultUrl("deepseek", "anthropic")).toBe(
      "https://api.deepseek.com/anthropic",
    );
    expect(providerProtocols("openrouter")).toEqual(["openai"]);
    expect(providerProtocols("custom")).toEqual(["openai", "anthropic"]);
  });

  it("provides the official default upstream address", () => {
    expect(defaultUpstreamUrl("openai")).toBe("https://api.openai.com/v1");
    expect(defaultUpstreamUrl("anthropic")).toBe("https://api.anthropic.com");
  });

  it("only enables regular-key balance queries for verified provider hosts", () => {
    expect(supportsBalanceQuery("https://api.deepseek.com/v1")).toBe(true);
    expect(supportsBalanceQuery("https://openrouter.ai/api/v1")).toBe(true);
    expect(supportsBalanceQuery("https://api.openai.com/v1")).toBe(false);
    expect(supportsBalanceQuery("not-a-url")).toBe(false);
  });

  it("compares the complete upstream network origin", () => {
    expect(sameUpstreamOrigin("https://api.example.test/v1", "https://api.example.test/v2")).toBe(true);
    expect(sameUpstreamOrigin("https://api.example.test", "https://other.example.test")).toBe(false);
    expect(sameUpstreamOrigin("http://127.0.0.1:8000", "http://127.0.0.1:9000")).toBe(false);
  });
});

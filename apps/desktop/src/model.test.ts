import { describe, expect, it } from "vitest";
import { defaultUpstreamUrl, localBaseUrl } from "./model";

describe("protocol URLs", () => {
  it("derives the protocol-specific local address", () => {
    expect(localBaseUrl("openai")).toBe("http://127.0.0.1:17891/v1");
    expect(localBaseUrl("anthropic")).toBe("http://127.0.0.1:17891");
  });

  it("provides the official default upstream address", () => {
    expect(defaultUpstreamUrl("openai")).toBe("https://api.openai.com/v1");
    expect(defaultUpstreamUrl("anthropic")).toBe("https://api.anthropic.com");
  });
});

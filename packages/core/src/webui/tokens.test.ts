import { describe, expect, it } from "vitest";
import { TokenIssuer } from "./tokens.js";

describe("TokenIssuer", () => {
  it("issues and checks tokens", () => {
    const issuer = new TokenIssuer(3600, 16);
    const issued = issuer.issue();
    expect(issued.token).toHaveLength(64);
    expect(issued.apiToken).toHaveLength(64);
    expect(issuer.checkWsToken(issued.token)).toBe(true);
    expect(issuer.checkApiToken(issued.apiToken)).toBe(true);
    expect(issuer.checkWsToken("bad")).toBe(false);
    expect(issuer.checkApiToken("bad")).toBe(false);
  });

  it("caps token count", () => {
    const issuer = new TokenIssuer(3600, 1);
    expect(issuer.tryIssue()).toBeDefined();
    expect(issuer.tryIssue()).toBeUndefined();
  });
});

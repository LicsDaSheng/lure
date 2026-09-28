import { describe, expect, it } from "vitest";

import { piDarkSyntaxTheme, piLightSyntaxTheme } from "./pi-syntax-theme";

describe("Markdown 代码语法主题", () => {
  it("为浅色和深色背景提供各自的高对比度配色", () => {
    expect(piLightSyntaxTheme.colors?.["editor.foreground"]).toBe("#1F2328");
    expect(piDarkSyntaxTheme.colors?.["editor.foreground"]).toBe("#E6EDF3");
    expect(piLightSyntaxTheme.tokenColors).not.toEqual(
      piDarkSyntaxTheme.tokenColors,
    );
  });

  it("浅色主题中的字符串、类型和注释不再使用暗色主题的浅色值", () => {
    expect(piLightSyntaxTheme.tokenColors).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          settings: expect.objectContaining({ foreground: "#0A3069" }),
        }),
        expect.objectContaining({
          settings: expect.objectContaining({ foreground: "#953800" }),
        }),
        expect.objectContaining({
          settings: expect.objectContaining({ foreground: "#57606A" }),
        }),
      ]),
    );
  });
});

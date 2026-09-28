import type { ThemeRegistrationAny } from "shiki";

/** 浅色代码块使用高对比度、低饱和的 GitHub 风格配色。 */
const piLightTokenColors: NonNullable<ThemeRegistrationAny["tokenColors"]> = [
  {
    scope: ["keyword", "storage", "storage.type"],
    settings: { foreground: "#6639BA" },
  },
  {
    scope: ["entity.name.function", "support.function"],
    settings: { foreground: "#8250DF" },
  },
  { scope: ["string", "string.quoted"], settings: { foreground: "#0A3069" } },
  { scope: ["constant.numeric"], settings: { foreground: "#0550AE" } },
  {
    scope: ["entity.name.type", "support.type", "support.class"],
    settings: { foreground: "#953800" },
  },
  {
    scope: ["comment", "punctuation.definition.comment"],
    settings: { foreground: "#57606A" },
  },
  {
    scope: ["variable", "variable.other"],
    settings: { foreground: "#1F2328" },
  },
  {
    scope: ["keyword.operator", "punctuation"],
    settings: { foreground: "#343942" },
  },
];

/** 深色模式单独配色，避免为照顾浅色背景而牺牲暗色可读性。 */
const piDarkTokenColors: NonNullable<ThemeRegistrationAny["tokenColors"]> = [
  {
    scope: ["keyword", "storage", "storage.type"],
    settings: { foreground: "#FF7B72" },
  },
  {
    scope: ["entity.name.function", "support.function"],
    settings: { foreground: "#D2A8FF" },
  },
  { scope: ["string", "string.quoted"], settings: { foreground: "#A5D6FF" } },
  { scope: ["constant.numeric"], settings: { foreground: "#79C0FF" } },
  {
    scope: ["entity.name.type", "support.type", "support.class"],
    settings: { foreground: "#FFA657" },
  },
  {
    scope: ["comment", "punctuation.definition.comment"],
    settings: { foreground: "#8B949E" },
  },
  {
    scope: ["variable", "variable.other"],
    settings: { foreground: "#E6EDF3" },
  },
  {
    scope: ["keyword.operator", "punctuation"],
    settings: { foreground: "#C9D1D9" },
  },
];

export const piLightSyntaxTheme: ThemeRegistrationAny = {
  name: "pi-light",
  type: "light",
  colors: { "editor.background": "#00000000", "editor.foreground": "#1F2328" },
  tokenColors: piLightTokenColors,
};

export const piDarkSyntaxTheme: ThemeRegistrationAny = {
  name: "pi-dark",
  type: "dark",
  colors: { "editor.background": "#00000000", "editor.foreground": "#E6EDF3" },
  tokenColors: piDarkTokenColors,
};

export const piSyntaxThemes: [ThemeRegistrationAny, ThemeRegistrationAny] = [
  piLightSyntaxTheme,
  piDarkSyntaxTheme,
];

import type { ThemeRegistrationAny } from "shiki";

const piTokenColors: NonNullable<ThemeRegistrationAny["tokenColors"]> = [
  { scope: ["keyword", "storage", "storage.type"], settings: { foreground: "#569CD6" } },
  { scope: ["entity.name.function", "support.function"], settings: { foreground: "#DCDCAA" } },
  { scope: ["string", "string.quoted"], settings: { foreground: "#CE9178" } },
  { scope: ["constant.numeric"], settings: { foreground: "#B5CEA8" } },
  { scope: ["entity.name.type", "support.type", "support.class"], settings: { foreground: "#4EC9B0" } },
  { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: "#6A9955" } },
  { scope: ["variable", "variable.other"], settings: { foreground: "#9CDCFE" } },
  { scope: ["keyword.operator", "punctuation"], settings: { foreground: "#D4D4D4" } },
];

export const piLightSyntaxTheme: ThemeRegistrationAny = {
  name: "pi-light",
  type: "light",
  colors: { "editor.background": "#00000000", "editor.foreground": "#252525" },
  tokenColors: piTokenColors,
};

export const piDarkSyntaxTheme: ThemeRegistrationAny = {
  name: "pi-dark",
  type: "dark",
  colors: { "editor.background": "#00000000", "editor.foreground": "#D4D4D4" },
  tokenColors: piTokenColors,
};

export const piSyntaxThemes: [ThemeRegistrationAny, ThemeRegistrationAny] = [
  piLightSyntaxTheme,
  piDarkSyntaxTheme,
];

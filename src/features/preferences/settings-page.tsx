import { ArrowLeftIcon, CheckIcon, MonitorIcon, MoonIcon, PaletteIcon, SunIcon } from "lucide-react";

import type { ThemePreference } from "./theme";

const themeOptions: Array<{
  value: ThemePreference;
  label: string;
  icon: typeof MonitorIcon;
}> = [
  { value: "system", label: "系统", icon: MonitorIcon },
  { value: "light", label: "浅色", icon: SunIcon },
  { value: "dark", label: "深色", icon: MoonIcon },
];

export function SettingsPage({
  onBack,
  onThemeChange,
  theme,
}: {
  onBack: () => void;
  onThemeChange: (theme: ThemePreference) => void;
  theme: ThemePreference;
}) {
  return (
    <div className="flex min-h-0 flex-1 overflow-hidden">
      <aside aria-label="设置导航" className="flex w-[272px] shrink-0 flex-col border-r border-border bg-sidebar px-3">
        <button
          className="mt-3 flex h-11 items-center gap-2 rounded-lg px-3 text-sm font-medium outline-none hover:bg-[var(--button-subtle-hover)] active:bg-[var(--button-subtle-active)] focus-visible:ring-[3px] focus-visible:ring-ring/50"
          onClick={onBack}
          type="button"
        >
          <ArrowLeftIcon aria-hidden="true" className="size-4" />
          返回应用
        </button>

        <nav aria-label="设置选项" className="mt-8">
          <p className="px-3 text-xs font-medium text-muted-foreground">设置</p>
          <div aria-current="page" className="mt-2 flex h-10 items-center gap-2 rounded-lg bg-[var(--button-subtle-active)] px-3 text-sm font-medium">
            <PaletteIcon aria-hidden="true" className="size-4" />
            外观
          </div>
        </nav>
      </aside>

      <main aria-label="外观设置" className="min-w-0 flex-1 overflow-y-auto bg-background">
        <div className="mx-auto w-full max-w-[1040px] px-10 py-16 lg:px-16">
          <h1 className="text-3xl font-semibold tracking-tight">外观</h1>
          <section aria-labelledby="theme-heading" className="mt-14">
            <h2 className="text-sm font-semibold" id="theme-heading">主题</h2>
            <div aria-label="主题" className="mt-5 grid max-w-[860px] grid-cols-1 gap-4 sm:grid-cols-3" role="radiogroup">
              {themeOptions.map((option) => (
                <ThemeOption
                  checked={theme === option.value}
                  icon={option.icon}
                  key={option.value}
                  label={option.label}
                  onSelect={() => onThemeChange(option.value)}
                  value={option.value}
                />
              ))}
            </div>
          </section>
        </div>
      </main>
    </div>
  );
}

function ThemeOption({
  checked,
  icon: Icon,
  label,
  onSelect,
  value,
}: {
  checked: boolean;
  icon: typeof MonitorIcon;
  label: string;
  onSelect: () => void;
  value: ThemePreference;
}) {
  return (
    <button
      aria-checked={checked}
      className="group min-w-0 rounded-xl text-left outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
      onClick={onSelect}
      role="radio"
      type="button"
    >
      <div className={`relative aspect-[1.42] overflow-hidden rounded-xl border-2 transition-colors ${checked ? "border-[#3B82F6]" : "border-border group-hover:border-muted-foreground/50"}`}>
        <ThemePreview value={value} />
        {checked && (
          <span className="absolute right-2 top-2 grid size-6 place-items-center rounded-full bg-[#3B82F6] text-white">
            <CheckIcon aria-hidden="true" className="size-4" strokeWidth={2.5} />
          </span>
        )}
      </div>
      <span className="mt-3 flex items-center justify-center gap-2 text-sm font-medium">
        <Icon aria-hidden="true" className="size-4 text-muted-foreground" />
        {label}
      </span>
    </button>
  );
}

function ThemePreview({ value }: { value: ThemePreference }) {
  const light = (
    <div className="flex h-full bg-[#F4F4F5] p-3">
      <div className="w-[29%] rounded-l-md bg-[#E5E5E7]" />
      <div className="flex flex-1 flex-col gap-2 rounded-r-md bg-white p-3">
        <span className="h-2 w-1/2 rounded-full bg-[#D2D2D5]" />
        <span className="h-2 w-3/4 rounded-full bg-[#E3E3E5]" />
        <span className="mt-auto h-1/3 rounded-md border border-[#E5E5E8] bg-[#F7F7F8]" />
      </div>
    </div>
  );
  const dark = (
    <div className="flex h-full bg-[#191919] p-3">
      <div className="w-[29%] rounded-l-md bg-[#333333]" />
      <div className="flex flex-1 flex-col gap-2 rounded-r-md bg-[#1F1F1F] p-3">
        <span className="h-2 w-1/2 rounded-full bg-[#5B5B5B]" />
        <span className="h-2 w-3/4 rounded-full bg-[#414141]" />
        <span className="mt-auto h-1/3 rounded-md border border-[#3A3A3A] bg-[#252525]" />
      </div>
    </div>
  );

  if (value === "light") return light;
  if (value === "dark") return dark;
  return (
    <div className="grid h-full grid-cols-2">
      <div className="overflow-hidden">{light}</div>
      <div className="overflow-hidden">{dark}</div>
    </div>
  );
}

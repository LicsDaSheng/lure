//! 旧配置格式迁移（对齐 crates/lure-core/src/config/migration.rs）。
//! 在原始 JSON 层就地变换，typed 解析之前应用。

function asObj(v: unknown): Record<string, unknown> | undefined {
  return v !== null && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : undefined;
}

/// 就地把旧格式配置迁移到当前格式。
export function migrateConfig(data: Record<string, unknown>): void {
  dropLegacyMaxMessages(data);
  migrateTools(data);
}

/// 丢弃 `agents.defaults.maxMessages` / `max_messages`。
function dropLegacyMaxMessages(data: Record<string, unknown>): void {
  const defaults = asObj(asObj(data["agents"])?.["defaults"]);
  if (defaults) {
    delete defaults["maxMessages"];
    delete defaults["max_messages"];
  }
}

function migrateTools(data: Record<string, unknown>): void {
  const tools = asObj(data["tools"]);
  if (!tools) return;

  // tools.exec.restrictToWorkspace → tools.restrictToWorkspace（目标不存在时才移动）。
  if (!("restrictToWorkspace" in tools)) {
    const exec = asObj(tools["exec"]);
    if (exec && "restrictToWorkspace" in exec) {
      tools["restrictToWorkspace"] = exec["restrictToWorkspace"];
      delete exec["restrictToWorkspace"];
    }
  }

  // tools.myEnabled/mySet → tools.my.{enable, allowSet}（已有子键优先）。
  const myEnabled = tools["myEnabled"];
  const mySet = tools["mySet"];
  delete tools["myEnabled"];
  delete tools["mySet"];

  if (myEnabled !== undefined || mySet !== undefined) {
    const my = asObj(tools["my"]) ?? {};
    if (myEnabled !== undefined && !("enable" in my)) my["enable"] = myEnabled;
    if (mySet !== undefined && !("allowSet" in my)) my["allowSet"] = mySet;
    tools["my"] = my;
  }
}

// 设置页数据层。对齐 lure-core::webui settings 契约：
//   GET  /api/settings                              → 完整载荷
//   GET  /api/settings/update?..                    → 编辑 agent 默认
//   GET  /api/settings/provider/update?..           → 写 provider api_key/api_base
//   GET  /api/settings/model-configurations/create?..
//   GET  /api/settings/model-configurations/update?..
// 写入均为 GET + query（snake_case）+ Bearer api_token，返回更新后的完整载荷。

export interface AgentSettings {
  model: string;
  provider: string;
  resolved_provider: string;
  has_api_key: boolean;
  model_preset: string;
  max_tokens: number;
  context_window_tokens: number;
  temperature: number;
  reasoning_effort: string | null;
}

export interface ModelPreset {
  name: string;
  label: string;
  active: boolean;
  is_default: boolean;
  model: string;
  provider: string;
  max_tokens: number;
  context_window_tokens: number;
  temperature: number;
  reasoning_effort: string | null;
}

export interface ProviderRow {
  name: string;
  label: string;
  configured: boolean;
  api_key_hint: string | null;
  api_base: string | null;
  default_api_base: string | null;
  enabled: boolean;
}

export interface Settings {
  agent: AgentSettings;
  model_presets: ModelPreset[];
  providers: ProviderRow[];
  version: { current: string };
  runtime: { workspace_path: string };
}

async function readOrThrow(res: Response): Promise<Settings> {
  const body = (await res.json().catch(() => ({}))) as
    | Settings
    | { error?: string };
  if (!res.ok) {
    const msg = (body as { error?: string }).error ?? `${res.status}`;
    throw new Error(msg);
  }
  return body as Settings;
}

export function fetchSettings(token: string): Promise<Settings> {
  return fetch("/api/settings", {
    headers: { Authorization: `Bearer ${token}` },
  }).then(readOrThrow);
}

/** 写入端点通用调用：GET + query（跳过 undefined），Bearer 鉴权，回完整载荷。 */
function writeUpdate(
  path: string,
  token: string,
  params: Record<string, string | number | undefined>,
): Promise<Settings> {
  const qs = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined) qs.set(k, String(v));
  }
  return fetch(`${path}?${qs.toString()}`, {
    headers: { Authorization: `Bearer ${token}` },
  }).then(readOrThrow);
}

export function updateAgent(
  token: string,
  params: {
    model?: string;
    provider?: string;
    context_window_tokens?: number;
    model_preset?: string;
  },
): Promise<Settings> {
  return writeUpdate("/api/settings/update", token, params);
}

export function updateProvider(
  token: string,
  params: { provider: string; api_key?: string; api_base?: string },
): Promise<Settings> {
  return writeUpdate("/api/settings/provider/update", token, params);
}

export function createPreset(
  token: string,
  params: { name: string; model: string; label?: string; provider?: string },
): Promise<Settings> {
  return writeUpdate(
    "/api/settings/model-configurations/create",
    token,
    params,
  );
}

export function updatePreset(
  token: string,
  params: {
    name: string;
    label?: string;
    provider?: string;
    model?: string;
    context_window_tokens?: number;
  },
): Promise<Settings> {
  return writeUpdate(
    "/api/settings/model-configurations/update",
    token,
    params,
  );
}

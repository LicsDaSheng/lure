import { test, expect } from "@playwright/test";

// 真实浏览器契约 smoke：加载 → bootstrap → echo 往返（WS）→ 删除真实 key（复现 URL 编码 bug）。
// 串行：test 2 创建的会话供 test 3 删除（后端状态跨用例持久）。
test.describe.serial("webui smoke (headless backend + echo)", () => {
  test("加载并完成 bootstrap，应用外壳渲染", async ({ page }) => {
    await page.goto("/");
    // 欢迎布局 heading 或 composer 输入框可见 → bootstrap 与加载期 /api 全部成功（否则页面会崩/白屏）。
    const shell = page
      .getByRole("heading", { name: "开始一段对话" })
      .or(page.getByRole("textbox", { name: "发消息给 Lure…" }));
    await expect(shell.first()).toBeVisible({ timeout: 15_000 });
  });

  test("echo 往返创建会话（走真实 WS mux + transcript）", async ({ page }) => {
    await page.goto("/");
    // 唯一的 composer 输入框（role=textbox，语言无关）；Enter（无 shift）发送。
    const input = page.getByRole("textbox").first();
    await expect(input).toBeVisible({ timeout: 15_000 });
    await input.fill("ping");
    await input.press("Enter");
    // EchoProvider 回显 "echo: ping"，证明 WS mux 往返 + transcript 落库。
    await expect(page.getByText("echo: ping").first()).toBeVisible({ timeout: 20_000 });
  });

  test("reload 后侧栏列出会话，webui-thread 用编码 key 取回历史", async ({ page }) => {
    // 依赖前一用例创建的会话（串行、后端状态持久）。reload 保证走真实 /api/sessions
    // 与 webui-thread 读取路径——后者曾因 key 未 URL 解码而静默丢历史（见 5142550）。
    await page.goto("/");
    // 侧栏空态文案消失 → 会话已落库并被 /api/sessions 列出。
    await expect(page.getByText("还没有对话")).toBeHidden({ timeout: 15_000 });

    // 在浏览器上下文用真实 encodeURIComponent 取 webui-thread，断言历史回得来。
    const thread = await page.evaluate(async () => {
      const boot = await (await fetch("/webui/bootstrap")).json();
      const headers = { Authorization: `Bearer ${boot.api_token}` };
      const list = await (await fetch("/api/sessions", { headers })).json();
      const key: string | undefined = list.sessions?.[0]?.key;
      if (!key) return { ok: false as const, reason: "无会话" };
      const res = await fetch(`/api/sessions/${encodeURIComponent(key)}/webui-thread`, { headers });
      const text = await res.text();
      return { ok: true as const, key, status: res.status, hasPing: text.includes("ping") };
    });
    expect(thread.ok, thread.ok ? undefined : thread.reason).toBeTruthy();
    if (!thread.ok) return;
    expect(thread.key).toContain(":");
    expect(thread.status).toBe(200);
    expect(thread.hasPing, "webui-thread 应含已发消息").toBe(true);
  });

  test("删除真实 session key 应 200（回归：URL 编码 %3A）", async ({ page }) => {
    await page.goto("/");
    // 直接在浏览器上下文用真实 encodeURIComponent + fetch，精确复现前端删除路径。
    const result = await page.evaluate(async () => {
      const boot = await (await fetch("/webui/bootstrap")).json();
      const headers = { Authorization: `Bearer ${boot.api_token}` };
      const before = await (await fetch("/api/sessions", { headers })).json();
      const key: string | undefined = before.sessions?.[0]?.key;
      if (!key) return { ok: false as const, reason: "无可删会话（echo 往返未落库？）" };
      const del = await fetch(`/api/sessions/${encodeURIComponent(key)}/delete`, { headers });
      const body = await del.json().catch(() => ({}));
      const after = await (await fetch("/api/sessions", { headers })).json();
      return {
        ok: true as const,
        key,
        status: del.status,
        deleted: body?.deleted,
        stillPresent: (after.sessions ?? []).some((s: { key: string }) => s.key === key),
      };
    });

    expect(result.ok, result.ok ? undefined : result.reason).toBeTruthy();
    if (!result.ok) return;
    expect(result.key, "真实 key 应含冒号（会被编码为 %3A）").toContain(":");
    expect(result.status).toBe(200);
    expect(result.deleted).toBe(true);
    expect(result.stillPresent, "删除后应从列表消失").toBe(false);
  });
});

// 设置页写入面契约：Rust 集成测试直连 http_server 模块，绕过 lure-desktop 全装配
// 里的 config_path 接线（save_config 落盘目标）；门禁要求经真实浏览器 + 真实后端装配
// 端到端验证 GET+query 写入语义、响应形状、以及重读确认落盘生效（--config 已隔离到
// workspace 内，不会污染真实用户配置）。
test.describe.serial("settings 写入面契约（headless backend + config-backed）", () => {
  test("agent update：改 context_window，200 + 响应回派生载荷 + 重读持久", async ({ page }) => {
    await page.goto("/");
    const result = await page.evaluate(async () => {
      const boot = await (await fetch("/webui/bootstrap")).json();
      const headers = { Authorization: `Bearer ${boot.api_token}` };
      // 前端以 GET .../update?a=b 携带 snake_case query（fetch 无 method 即 GET）。
      const q = new URLSearchParams({ context_window_tokens: "123456" }).toString();
      const put = await fetch(`/api/settings/update?${q}`, { headers });
      const body = await put.json();
      // 重读 /api/settings 证明变更已 save_config 落盘并被后端重新派生。
      const reread = await (await fetch("/api/settings", { headers })).json();
      return {
        status: put.status,
        written: body?.agent?.context_window_tokens,
        persisted: reread?.agent?.context_window_tokens,
      };
    });
    expect(result.status).toBe(200);
    expect(result.written).toBe(123456);
    expect(result.persisted, "重读应见落盘后的新值").toBe(123456);
  });

  test("preset create → agent 指向新 preset，均 200 且持久", async ({ page }) => {
    await page.goto("/");
    const result = await page.evaluate(async () => {
      const boot = await (await fetch("/webui/bootstrap")).json();
      const headers = { Authorization: `Bearer ${boot.api_token}` };
      const createQ = new URLSearchParams({
        name: "e2e-fast",
        label: "E2E Fast",
        provider: "auto",
        model: "echo/echo",
      }).toString();
      const create = await fetch(`/api/settings/model-configurations/create?${createQ}`, { headers });
      const createBody = await create.json();
      // model_presets 是行数组（每行含 name），非 keyed object。
      const rows: Array<{ name?: string }> = createBody?.model_presets ?? [];
      // 让默认 agent 指向刚建的命名 preset。
      const pointQ = new URLSearchParams({ model_preset: "e2e-fast" }).toString();
      const point = await fetch(`/api/settings/update?${pointQ}`, { headers });
      const pointBody = await point.json();
      const reread = await (await fetch("/api/settings", { headers })).json();
      return {
        createStatus: create.status,
        hasPreset: rows.some((p) => p.name === "e2e-fast"),
        pointStatus: point.status,
        activePreset: pointBody?.agent?.model_preset,
        persistedPreset: reread?.agent?.model_preset,
      };
    });
    expect(result.createStatus).toBe(200);
    expect(result.hasPreset, "响应 model_presets 应含新建条目").toBe(true);
    expect(result.pointStatus).toBe(200);
    expect(result.activePreset).toBe("e2e-fast");
    expect(result.persistedPreset, "重读应见 agent 指向新 preset").toBe("e2e-fast");
  });

  test("非法 context_window（非数字）应 400 且不落盘", async ({ page }) => {
    await page.goto("/");
    const result = await page.evaluate(async () => {
      const boot = await (await fetch("/webui/bootstrap")).json();
      const headers = { Authorization: `Bearer ${boot.api_token}` };
      const before = await (await fetch("/api/settings", { headers })).json();
      const q = new URLSearchParams({ context_window_tokens: "lots" }).toString();
      const bad = await fetch(`/api/settings/update?${q}`, { headers });
      const badBody = await bad.json().catch(() => ({}));
      const after = await (await fetch("/api/settings", { headers })).json();
      return {
        status: bad.status,
        hasError: typeof badBody?.error === "string",
        unchanged:
          before?.agent?.context_window_tokens === after?.agent?.context_window_tokens,
      };
    });
    expect(result.status).toBe(400);
    expect(result.hasError, "400 应回 JSON error 字段").toBe(true);
    expect(result.unchanged, "校验失败不得落盘").toBe(true);
  });
});

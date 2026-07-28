import { test, expect } from "@playwright/test";

// 真实浏览器契约 smoke：加载 → bootstrap → echo 往返（WS）→ 删除真实 key（复现 URL 编码 bug）。
// 串行：test 2 创建的会话供 test 3 删除（后端状态跨用例持久）。
test.describe.serial("webui smoke (headless backend + echo)", () => {
  test("加载并完成 bootstrap，应用外壳渲染", async ({ page }) => {
    await page.goto("/");
    // composer dock 或欢迎布局可见 → bootstrap 与加载期 /api 全部成功（否则页面会崩/白屏）。
    const shell = page
      .getByTestId("thread-composer-dock")
      .or(page.getByTestId("thread-welcome-layout"));
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
    await expect(page.getByText("No sessions yet.")).toBeHidden({ timeout: 15_000 });

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

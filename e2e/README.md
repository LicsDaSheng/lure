# Lure E2E（真实浏览器契约测试）

用 Playwright 驱动**真实 Chromium** 打 **headless `lure-desktop`**（`--model echo`），
验证前后端契约——`/api` 端点、URL 编码、WebSocket、bootstrap——这些是 Rust 单元
测试（字面 key、无浏览器）照不出的盲区。本次的删除 404 bug（前端 `encodeURIComponent`
把 `:` 编码为 `%3A`，服务端未解码）正属此类。

独立于 vendor 的 `frontend/webui`，不污染上游源码。

## 一次性准备

```bash
cd e2e
bun install                    # 或 npm install
bunx playwright install chromium
```

## 运行

```bash
cd e2e
bun run e2e                    # 无头运行全部 smoke
bun run e2e:ui                 # Playwright UI 模式
bun run e2e:report            # 打开上次 HTML 报告
```

`webServer` 会自动：
0. 先清理占用该端口的残留进程（被中断的 headless 会一直 park 占端口）
1. `cd frontend/webui && bun run build`（产出 `frontend/dist` 供 rust-embed 嵌入）
2. `cargo run -p lure-desktop -- --headless --model echo --http-port 8788`

HTTP 端口**固定**为 8788（Playwright 会多次加载 config，动态端口会导致 webServer 与
测试 baseURL 取到不同端口）。改端口：

```bash
LURE_E2E_PORT=9000 bun run e2e
```

首次含 Rust 编译，可能数分钟（timeout 300s）。后端 workspace 落在 `e2e/.workspace`（已 gitignore）。

## 覆盖

`tests/smoke.spec.ts`（串行）：
1. 加载 + bootstrap → 应用外壳渲染（catch 加载期 /api 契约破裂）。
2. echo 往返 → 创建会话（走真实 WS mux + transcript）。
3. 删除真实 `websocket:` key → 断言 200 + 消失（**回归**本次 URL 编码 bug）。

## 何时跑

涉及 `webui` HTTP/WS 契约、前端 vendor 同步、或响应形状/编码的改动，除 Rust 门禁外
应过本 smoke。

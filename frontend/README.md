# Frontend

Lure 桌面 WebUI 的前端源码。

- `app/`：自有 WebUI，React 18 + shadcn/ui + Tailwind v4（Vite 构建）。对接 `lure-core::webui`
  的后端契约（HTTP `/api/*`、`/webui/bootstrap`，WS 复用协议），非 vendored。
- `dist/`：构建产物（gitignore），由 `lure-desktop` 经 rust-embed 嵌入二进制。
- `nanobot/`、`UPSTREAM_COMMIT`：旧 vendored nanobot 前端的遗留物（`webui/` 已移除），
  仅历史参考，不参与构建。

## 构建

```bash
cd frontend/app
bun install
bun run build          # 产物输出到 ../dist（vite.config 已配 outDir + emptyOutDir）
```

## 开发热更

```bash
# 终端 1：起 headless 后端
cargo run --bin lure-desktop -- --headless --http-port 1789 --model echo

# 终端 2：Vite dev server（/webui、/api 代理到后端；WS 走 bootstrap 返回的绝对地址）
cd frontend/app && LURE_BACKEND=http://127.0.0.1:1789 bun run dev
```

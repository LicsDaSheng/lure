# Phase 10 Desktop WebUI 实施计划

> 目标：lure desktop 应用——原样复用 nanobot React WebUI（交互/渲染一致），
> 能力由 Rust desktop 进程内提供（HTTP REST + WS 复用协议），不跑独立 web 服务。

**Tech Stack**：Rust workspace（lure-core / lure-cli / 新增 lure-desktop）、tiny_http、tungstenite、
wry+tao（macOS WKWebView）、前端 bun + Vite + React 18（vendor 自上游）。

**上游基线**：nanobot commit `4cd6eb6`（2026-07-22，webui/ 最后改动）。

## 架构决策

1. **前端原样 vendor**：拷贝上游 `webui/` → `frontend/webui`（不含 node_modules），
   `bun run build` → `frontend/dist`，经 `rust-embed` 嵌入 desktop 二进制。不改前端代码，
   保证交互/渲染与 nanobot 一致。
2. **进程内 loopback server**：desktop 进程启动时绑定 `127.0.0.1:<临时端口>`，
   提供静态资源 + `/webui/bootstrap` + `/api/*` + WebSocket 升级。
   这是嵌入式能力（非独立 gateway 进程），且是前端**零改动**的唯一方式——
   前端所有 fetch 走 same-origin 相对路径（`api.ts`、`bootstrap.ts`）。
3. **WS 复用协议走真实 WebSocket**（tungstenite）：前端默认连接路径
   （`deriveWsUrl` → `ws://127.0.0.1:<port><ws_path>?token=`）。
   上游前端还原生支持 `nanobot-host://` IPC 桥（`window.nanobotHost`，见 `runtime.ts`），
   后续可用它替换 TCP 依赖，前端仍零改动——记入后续切片，不在本次。
4. **传输无关协议层先进 lure-core**（TDD），传输层（tiny_http/tungstenite/wry）最后接线。

## 启动链路（上游事实）

1. `GET /webui/bootstrap`（可带 `X-Nanobot-Auth`；localhost-only）→
   `{token, ws_path, api_token, ws_url?, expires_in, model_name?, limits?, runtime_surface?, runtime_capabilities?}`
2. 前端 `deriveWsUrl(ws_path, token)` → `ws://<host><ws_path>?token=`；`api_token` 用于
   `/api/*` 的 `Authorization: Bearer <token>`。
3. WS 连接后服务端发 `{"event":"ready","chat_id":<uuid>,"client_id":<id>}` 并自动 attach 默认会话。
4. 客户端帧：`{"type":"attach","chat_id"}` / `{"type":"new_chat"}` /
   `{"type":"message","chat_id","content","webui":true,"turn_id"?}`。
5. 服务端事件（精确形状，上游 `channels/websocket/runtime.py`）：
   - `attached`：`{"event":"attached","chat_id"}`
   - `delta`：`{"event":"delta","chat_id","text","stream_id"?}`
   - `stream_end`：`{"event":"stream_end","chat_id","text"?,"stream_id"?}`
   - `reasoning_delta` / `reasoning_end`：同 delta 形状（无 text 为 end）
   - `message`（最终回复）：`{"event":"message","chat_id","text","latency_ms"?,"tool_events"?,"kind"?}`
   - `turn_end`：`{"event":"turn_end","chat_id","latency_ms"?,"goal_state"?}`
   - `goal_status`：`{"event":"goal_status","chat_id","status","started_at"?}`（running 带 started_at）
   - `session_updated`：`{"event":"session_updated","chat_id","scope"?}`（广播所有连接）
   - `error`：`{"event":"error","detail",...}`（invalid chat_id / missing content / unknown type）
   - `runtime_model_updated`：`{"event":"runtime_model_updated","model_name","model_preset"?}`

## 最小 REST 面（本次）

| 端点 | 用途 | 复用 |
|---|---|---|
| `GET /webui/bootstrap` | 启动握手 | 新实现（对齐 `webui/ws_http.py::_handle_bootstrap`） |
| `GET /api/sessions` | 会话列表 `{sessions:[{key,created_at,updated_at,title,preview}]}` | `webui::session_index` |
| `GET /api/sessions/{key}/webui-thread` | 消息视图（404 → null） | `webui::thread` |
| `DELETE /api/sessions/{key}` | 删除会话 | session 存储 |
| 静态资源 `/*` | index.html + assets + SPA fallback | rust-embed 内嵌 |

启动期前端还会调用 `/api/settings`、`/api/webui/sidebar-state` 等——以实测为准逐个补
stub（空载荷/默认值），记入 ledger；不在本次真实化。

## 任务切片（每片 TDD + 独立提交）

- **Task 0（10c）前端 vendor**：拷贝 + bun 构建 + 构建产物验收（dist/index.html 存在、
  上游 vitest 可选）。
- **Task 1（10a）`lure-core::webui::mux`**：复用协议会话 handler（传输无关）。
  `MuxSession`：new → ready 帧；handle_frame 按 type 分发（attach/new_chat/message/未知）；
  message 经注入的同步 turn executor 驱动（fake 覆盖），事件帧按上游形状产出。
- **Task 2（10b）`lure-core::webui::http`**：bootstrap（token 签发/校验、localhost-only、
  model_name 来自 resolver）+ `/api/sessions` + `webui-thread` + DELETE + 静态资源/SPA fallback。
  复用 Phase 9 tiny_http 模式。
- **Task 3（10d）WS transport**：tungstenite 升级 `<ws_path>?token=`，文本帧 ↔ MuxSession。
- **Task 4（10e）`lure-desktop` crate**：wry+tao 窗口加载 loopback URL，rust-embed 静态资源，
  `cargo run --bin lure-desktop` 端到端验收（列表 → 新会话 → 流式回复 → 刷新持久化）。

## 明确不做（本次）

- settings/skills/commands/automations/mcp/media/transcription 等 /api 大表面（stub 起步）。
- `nanobot-host://` IPC 桥、自定义协议（无 TCP）模式。
- workspace scope、fork_chat、goal_state 持久化、transcript 磁盘记录。
- 打包/签名/自动更新。

## 验收

- 每片：`rtk cargo fmt --check` + `clippy -D warnings` + `cargo test --all-targets --all-features`。
- 最终：desktop 窗口内完成「会话列表 → 新会话 → 流式对话 → 重开窗口历史仍在」。
- ledger 更新上游 `tests/webui/` 与 `webui/src/tests/` 的覆盖映射与缺口。

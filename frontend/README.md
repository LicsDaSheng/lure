# Frontend（vendor 自上游 nanobot）

本目录原样拷贝上游 nanobot 的 WebUI 前端源码，**不修改交互与渲染**。

- 上游仓库：`/Users/scottlee/workspace/github/nanobot`
- 基线 commit：见 [UPSTREAM_COMMIT](./UPSTREAM_COMMIT)（同步时更新此文件）
- `webui/`：上游 `webui/` 完整拷贝（不含 `node_modules/`）
- `nanobot/channels/*/webui/`：上游 channel UI 贡献（前端 `import.meta.glob`
  以 `../../../nanobot/channels/*/webui/**` 相对路径引用，目录结构必须保留）

## 构建

```bash
cd frontend/webui
bun install
bun run build -- --outDir ../dist --emptyOutDir
```

产物输出到 `frontend/dist/`，由 `lure-desktop` 经 rust-embed 嵌入二进制。

## 同步上游

重新执行拷贝后运行 `bun run test` 验证 vendor 完整性，并更新 UPSTREAM_COMMIT。

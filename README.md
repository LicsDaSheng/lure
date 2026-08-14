# Lure

用 TypeScript 实现的轻量 AI agent 框架，后端行为对齐 [`nanobot`](https://github.com/HKUDS/nanobot)。

> 本仓库当前处于 **Rust → TypeScript 技术栈改道**阶段（分支 `refactor/typescript-rewrite`）。
> 产品特性能力不变，仅替换实现技术栈。技术方案见 [handbook/typescript-migration-plan.md](handbook/typescript-migration-plan.md)。

## 目录

```
desktop/   桌面产品入口（ui / shell / backend）
packages/  核心能力（schema / core / server）
e2e/       Playwright 契约测试
```

## 技术栈

Node 22 · pnpm workspaces · Hono · zod v4 · assistant-ui + shadcn + Tailwind · Tauri V2 壳 · vitest + Playwright

## 约定

- 中文沟通、提交与文档。
- 详见 [AGENTS.md](AGENTS.md)。

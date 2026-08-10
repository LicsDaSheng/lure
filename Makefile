# Lure 本地门禁。执行 `make` 查看全部目标。
# 说明：Makefile 用原生 cargo/bun（不含个人代理），便于任何环境直接跑。

.DEFAULT_GOAL := help
.PHONY: help fmt fmt-check lint test rust e2e-setup e2e check all clean-e2e

help: ## 列出可用目标
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

## ---- Rust 门禁 ----

fmt: ## 格式化全 workspace
	cargo fmt --all

fmt-check: ## 校验格式（不改动）
	cargo fmt --all --check

lint: ## clippy（warnings 即错误）
	cargo clippy --all-targets --all-features -- -D warnings

test: ## 全量测试
	cargo test --all-targets --all-features

rust: fmt-check lint test ## Rust 完整门禁：fmt-check + lint + test

## ---- E2E（真实浏览器契约测试）----

e2e-setup: ## 安装 E2E 依赖（前端 + Playwright + Chromium）
	cd frontend/app && bun install
	cd e2e && bun install && bunx playwright install chromium

e2e: ## 运行 Playwright 契约 smoke（自动构建 dist + 起 headless 后端）
	cd e2e && bun run e2e

## ---- 组合 ----

check: rust e2e ## 完整门禁：Rust + E2E（提交前跑）

all: check ## check 的别名

clean-e2e: ## 清理 E2E 产物与后端 workspace
	rm -rf e2e/test-results e2e/playwright-report e2e/.workspace frontend/dist

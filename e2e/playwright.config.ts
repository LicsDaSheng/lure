import { defineConfig, devices } from "@playwright/test";
import path from "node:path";
import { fileURLToPath } from "node:url";

// __dirname 在 ESM 配置下的等价物：定位 e2e/ 与仓库根。
const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");

// HTTP 端口**必须固定**：Playwright 会多次加载本 config（主进程 + worker），
// 动态端口会导致 webServer 与测试 baseURL 取到不同端口。用 LURE_E2E_PORT 覆盖。
// webServer 启动前先清理占用该端口的残留进程（被中断的 headless 会一直 park）。
const PORT = Number(process.env.LURE_E2E_PORT ?? 8788);
const BASE_URL = `http://127.0.0.1:${PORT}`;

// 后端 workspace（gitignored）；--model echo 使整条链路离线、确定性。
const WORKSPACE = path.join(here, ".workspace");

export default defineConfig({
  testDir: path.join(here, "tests"),
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? "line" : [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: BASE_URL,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    // 先用 vendored 前端产出 frontend/dist（供 lure-desktop rust-embed 嵌入），
    // 再拉起 headless 后端（同一套生产 server 装配，仅无窗口）。
    command:
      `(lsof -ti tcp:${PORT} | xargs kill -9 2>/dev/null || true); ` +
      `(cd frontend/webui && bun run build -- --outDir ../dist --emptyOutDir) && ` +
      `cargo run --quiet -p lure-desktop -- --headless --model echo ` +
      `--http-port ${PORT} --workspace ${WORKSPACE}`,
    cwd: repoRoot,
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 300_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});

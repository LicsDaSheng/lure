import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

// dev 后端地址：`lure-desktop --headless --http-port 1789` 打印 LURE_HTTP_URL。
// WS 走 bootstrap 返回的绝对 ws_url（独立端口），无需在此代理。
const BACKEND = process.env.LURE_BACKEND ?? "http://127.0.0.1:1789";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "src") },
  },
  server: {
    proxy: {
      "/webui": { target: BACKEND, changeOrigin: true },
      "/api": { target: BACKEND, changeOrigin: true },
    },
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
});

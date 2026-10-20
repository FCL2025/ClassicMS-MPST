import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const hostedApi = fileURLToPath(new URL("./src/api.hosted.ts", import.meta.url));

// Hosted（靜態資料集）版建置設定：與 vite.config.web.ts 同款，差異在
// 1. alias：把 App/components 的 "./api" / "../api" 換成 src/api.hosted.ts
//    （雙後端委派層：預設本機資料夾 WASM worker，使用者可切換雲端 HTTP fetch）。
// 2. define：資料集根 URL 與版本，可用環境變數覆寫。預設值依 command 區分：
//    dev 用 scripts/serve_hosted.mjs 的 http://localhost:8788；
//    build 用同源 "/data"（部署後由 workers/hosted.js 從 R2 取出）。
//    環境變數 VITE_HOSTED_DATA_BASE 永遠最優先。
// 3. 獨立 outDir dist-hosted/，不覆蓋桌面版 dist/ 與 web 版 dist-web/。
export default defineConfig(({ command }) => ({
  plugins: [react()],
  clearScreen: false,
  define: {
    __WEB_BUILD__: "true",
    __HOSTED_DATA_BASE__: JSON.stringify(
      process.env.VITE_HOSTED_DATA_BASE ??
        (command === "build" ? "/data" : "http://localhost:8788"),
    ),
    __HOSTED_DATA_VERSION__: JSON.stringify(
      process.env.VITE_HOSTED_DATA_VERSION ?? "v20261020",
    ),
  },
  resolve: {
    alias: [{ find: /^\.{1,2}\/api$/, replacement: hostedApi }],
  },
  server: {
    port: 1422,
    strictPort: true,
    watch: {
      // 與桌面版設定同：排除 cargo/建置產物，避免監看事件灌爆 chokidar。
      ignored: [
        "**/src-tauri/**",
        "**/target/**",
        "**/dist/**",
        "**/dist-web/**",
        "**/dist-hosted/**",
        "**/out/**",
        "**/web-wasm/pkg/**",
      ],
    },
  },
  optimizeDeps: {
    // 同桌面版：只吃根目錄 index.html，不要爬 dist/、target/ 下的 html。
    entries: ["index.html"],
  },
  worker: {
    // 本機資料夾模式（api.web.ts）會打包 src/web/worker.ts + wasm glue，
    // 與 vite.config.web.ts 相同需求：ES module 格式（wasm glue 需要 import.meta）。
    format: "es",
  },
  build: {
    outDir: "dist-hosted",
    target: "es2022",
  },
}));

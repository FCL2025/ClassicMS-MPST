import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const webApi = fileURLToPath(new URL("./src/api.web.ts", import.meta.url));

// Web(純前端 WASM)版建置設定:與 vite.config.ts 同款,差異只在
// 1. alias:把 App/components 的 "./api" / "../api" 換成 src/api.web.ts
//    (api.web.ts 內部不 import "./api",不會自我循環)。
// 2. 獨立 outDir,不覆蓋桌面版用的 dist/。
// 3. Worker 以 ES module 格式打包(wasm glue 需要 import.meta)。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: {
    __WEB_BUILD__: "true",
  },
  resolve: {
    alias: [{ find: /^\.{1,2}\/api$/, replacement: webApi }],
  },
  server: {
    port: 1421,
    strictPort: true,
    watch: {
      // 與桌面版設定同：排除 cargo/建置產物，避免監看事件灌爆 chokidar。
      ignored: [
        "**/src-tauri/**",
        "**/target/**",
        "**/dist/**",
        "**/dist-web/**",
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
    format: "es",
  },
  build: {
    outDir: "dist-web",
    target: "es2022",
  },
});

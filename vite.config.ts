import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 開發慣例：固定 1420 port、clearScreen 關閉
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: {
    __WEB_BUILD__: "false",
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Tauri 官方建議：不要讓 Vite 監看 src-tauri，否則每次 cargo 編譯寫入的
      // 產物都會觸發檔案監看事件。注意 cargo 以工作區根目錄的 target/ 為輸出
      // （不在 src-tauri/ 內），所以必須一併忽略根目錄 target/，否則編譯期間
      // 數萬個檔案事件會灌爆 chokidar，甚至造成 dev server 誤判 config 變更重啟。
      // dist/dist-web/out/web-wasm pkg 同為建置/匯出產物，一併排除。
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
    // 預設會爬專案根下所有 **/*.html 當掃描入口，連 dist-web/ 與
    // target/**/tauri-codegen-assets/*.html 都會被掃進去；掃描 target/ 動輒
    // 數十秒，期間若 server 重啟就會噴「Failed to scan for dependencies」。
    // 明確指定唯一入口 index.html。
    entries: ["index.html"],
  },
  build: {
    outDir: "dist",
    target: "es2022",
  },
});

// Hosted 模式一鍵開發：同時起資料集靜態伺服器（8788）與 vite dev server（1422）。
// 任一子程序結束就整組關掉。不加新 npm 依賴；vite 直接跑 node_modules 裡的 bin，
// 避免 Windows 上 npx.cmd 包一層造成 process tree 收不掉。
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const viteBin = fileURLToPath(new URL("../node_modules/vite/bin/vite.js", import.meta.url));

const children = [];
let shuttingDown = false;

function shutdown(code) {
  if (shuttingDown) return;
  shuttingDown = true;
  for (const child of children) {
    if (!child.killed) child.kill("SIGTERM");
  }
  // 給子程序一點時間收尾，然後強制結束主程序。
  setTimeout(() => process.exit(code), 300).unref();
}

function run(label, args) {
  const child = spawn(process.execPath, args, { stdio: "inherit" });
  children.push(child);
  child.on("exit", (code, signal) => {
    console.log(`[dev:hosted] ${label} 結束（${signal ?? `code ${code}`}），整組關閉`);
    shutdown(code ?? 0);
  });
  child.on("error", (error) => {
    console.error(`[dev:hosted] ${label} 啟動失敗：${error.message}`);
    shutdown(1);
  });
}

run("資料伺服器", [fileURLToPath(new URL("./serve_hosted.mjs", import.meta.url))]);
run("vite", [viteBin, "--config", "vite.config.hosted.ts"]);

process.on("SIGINT", () => shutdown(0));
process.on("SIGTERM", () => shutdown(0));

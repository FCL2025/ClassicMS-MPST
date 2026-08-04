// 上傳 hosted 資料集到 Cloudflare R2（bucket: msclassic-data）。
//
// 用法：node scripts/upload_hosted.mjs <version> [--dry-run]
//   例：node scripts/upload_hosted.mjs v20260804 --dry-run
//
// 行為：
//   1. 掃描 out/hosted/<version>/ 下全部檔案，R2 key = "<version>/<posix 相對路徑>"。
//   2. 產生 `wrangler r2 bulk put` 用的 JSON manifest（陣列，entry 為
//      { "key": ..., "file": <絕對路徑> }），每批 1,000 筆切成 chunk，
//      寫到 out/hosted/.upload/chunk-*.json。
//   3. 逐批執行 wrangler r2 bulk put（低並發 5，避免 R2 429 速率限制）。
//      bulk put 一遇錯會中止該批剩餘檔案，所以每批失敗會以指數退避重試
//      （重複上傳是覆寫，安全）。整批跑完仍有失敗時非零結束，
//      直接重跑本腳本即可（已上傳的會被覆寫，結果相同）。
//   --dry-run：只產 manifest 與統計，不呼叫 wrangler。
import { readdir, mkdir, writeFile, readFile } from "node:fs/promises";
import { join, resolve, sep, basename } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";

const CHUNK_SIZE = 1000;
const BUCKET = "msclassic-data";
const CONCURRENCY = 5;
const MAX_ATTEMPTS = 6;

const version = process.argv[2];
const dryRun = process.argv.includes("--dry-run");
if (!version || version.startsWith("--")) {
  console.error("用法：node scripts/upload_hosted.mjs <version> [--dry-run]");
  process.exit(1);
}

const DATA_DIR = resolve(fileURLToPath(new URL(`../out/hosted/${version}`, import.meta.url)));
const UPLOAD_DIR = resolve(fileURLToPath(new URL("../out/hosted/.upload", import.meta.url)));

/** 遞迴收集目錄下所有檔案的絕對路徑。 */
async function collectFiles(dir) {
  const out = [];
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...(await collectFiles(full)));
    } else if (entry.isFile()) {
      out.push(full);
    }
  }
  return out;
}

/** 執行一批 bulk put；成功回 true。 */
function putChunk(chunkPath) {
  return new Promise((resolvePromise) => {
    // 直接跑 node_modules 裡的 wrangler.js：Windows 上 spawn npx.cmd 會被
    // Node 的 batch 檔保護擋下（spawn EINVAL），比照 dev_hosted.mjs 對 vite 的做法。
    const wranglerJs = resolve(
      fileURLToPath(new URL("../node_modules/wrangler/bin/wrangler.js", import.meta.url)),
    );
    const child = spawn(
      process.execPath,
      [
        wranglerJs, "r2", "bulk", "put", BUCKET,
        "--remote", "-y",
        "-f", chunkPath,
        "--concurrency", String(CONCURRENCY),
      ],
      { stdio: "inherit" },
    );
    child.on("error", () => resolvePromise(false));
    child.on("close", (code) => resolvePromise(code === 0));
  });
}

const files = (await collectFiles(DATA_DIR)).sort();
if (files.length === 0) {
  console.error(`[upload] 找不到資料：${DATA_DIR}`);
  process.exit(1);
}

const entries = files.map((file) => ({
  key: `${version}/${file.slice(DATA_DIR.length + 1).split(sep).join("/")}`,
  file,
}));

await mkdir(UPLOAD_DIR, { recursive: true });

// 斷點續傳：done-<version>.json 記錄「批次檔名 → 內容 sha1」，重跑時
// 內容沒變的已完成批次直接跳過；pipeline 重跑導致內容變了的批次會重傳。
const DONE_FILE = join(UPLOAD_DIR, `done-${version}.json`);
let done = {};
try {
  done = JSON.parse(await readFile(DONE_FILE, "utf8"));
} catch {
  done = {};
}

const chunkPaths = [];
const chunkHashes = new Map();
for (let i = 0; i < entries.length; i += CHUNK_SIZE) {
  const chunk = entries.slice(i, i + CHUNK_SIZE);
  const chunkJson = JSON.stringify(chunk);
  const chunkPath = join(UPLOAD_DIR, `chunk-${String(chunkPaths.length).padStart(3, "0")}.json`);
  await writeFile(chunkPath, chunkJson);
  chunkHashes.set(basename(chunkPath), createHash("sha1").update(chunkJson).digest("hex"));
  chunkPaths.push(chunkPath);
}
const skipped = chunkPaths.filter((p) => done[basename(p)] === chunkHashes.get(basename(p)));
console.log(
  `[upload] ${version}：共 ${entries.length} 筆，切成 ${chunkPaths.length} 批（每批 ${CHUNK_SIZE}），manifest 在 ${UPLOAD_DIR}`,
);
if (skipped.length > 0) console.log(`[upload] 斷點續傳：跳過 ${skipped.length} 批已完成的`);

if (dryRun) {
  console.log("[upload] --dry-run：只產 manifest 與統計，不呼叫 wrangler。");
  process.exit(0);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** 單批上傳，失敗以指數退避重試（429 速率限制需要等配額恢復）。 */
async function putChunkWithRetry(chunkPath, label) {
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
    if (attempt > 1) {
      const waitSec = 15 * (attempt - 1);
      console.log(`[upload] ${label} 第 ${attempt}/${MAX_ATTEMPTS} 次嘗試（等 ${waitSec}s 讓速率限制恢復）…`);
      await sleep(waitSec * 1000);
    }
    if (await putChunk(chunkPath)) return true;
    console.log(`[upload] ${label} 第 ${attempt} 次失敗`);
  }
  return false;
}

const failed = [];
for (let i = 0; i < chunkPaths.length; i++) {
  const name = basename(chunkPaths[i]);
  if (done[name] === chunkHashes.get(name)) continue; // 已完成，跳過
  console.log(`[upload] 批次 ${i + 1}/${chunkPaths.length}：${chunkPaths[i]}`);
  if (await putChunkWithRetry(chunkPaths[i], `批次 ${i + 1}`)) {
    done[name] = chunkHashes.get(name);
    await writeFile(DONE_FILE, JSON.stringify(done));
  } else {
    failed.push(chunkPaths[i]);
  }
  await sleep(2000); // 批次間留一點間隔，降低觸發 429 的機率
}
if (failed.length > 0) {
  console.error(`[upload] 仍有 ${failed.length} 批失敗：`);
  for (const p of failed) console.error(`  ${p}`);
  console.error("[upload] 直接重跑本腳本即可，已完成的批次會自動跳過。");
  process.exit(1);
}
console.log("[upload] 全部上傳完成。");

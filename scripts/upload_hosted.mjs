// 上傳 hosted 資料集到 Cloudflare R2（bucket: msclassic-data）。
//
// 用法：node scripts/upload_hosted.mjs <version> [選項]
//   例：node scripts/upload_hosted.mjs v20260804
//       node scripts/upload_hosted.mjs v20260804 --dry-run
//       node scripts/upload_hosted.mjs v20260804 --concurrency 64
//
// 選項：
//   --dry-run          只掃描與統計，不真的上傳。
//   --force            忽略續傳紀錄，全部重傳。
//   --concurrency N    S3 路徑的最大並發（預設 48，會依 429/5xx 自動調降）。
//   --sample N         只傳 N 個檔案（在全部待傳檔案裡等距取樣，涵蓋各種副檔名／目錄）。
//                      給正式上傳前的小範圍實測用。
//   --rest             強制走舊的 wrangler REST 路徑（除錯用）。
//
// ── 為什麼有兩條路徑 ────────────────────────────────────────────────
// 1.（預設，快）S3 相容 API：直接 PUT 到 <account>.r2.cloudflarestorage.com。
//    R2 對這個介面沒有帳號級的請求數上限，唯一相關的限制是
//    「同一個 object key 每秒最多 1 次寫入」——我們每個 key 只寫一次，不會踩到。
//    真正的瓶頸是本機上傳頻寬，所以可以開高並發。
//    需要 R2 API token（見下方 .env 說明）。
//
// 2.（備援，慢）wrangler r2 bulk put：走 Cloudflare REST API
//    （api.cloudflare.com/.../r2/buckets/.../objects/...）。
//    該介面全帳號硬上限 1,200 requests / 5 分鐘 ≈ 4 req/s，
//    而且一旦超過，接下來 5 分鐘所有 API 呼叫都會被擋（HTTP 429）。
//    本資料集約 3 萬個檔案 → 光是速率限制就要 2 小時以上，這就是原本很慢的原因。
//    調 --concurrency 對它沒用（限制是固定速率，不是並發數）。
//    只有在沒設定 R2 API token 時才會走這條，並且會刻意把速度壓在 4 req/s 以下。
//    參考：https://developers.cloudflare.com/r2/platform/limits/#cloudflare-rest-api
//
// ── 設定 R2 API token（一次性）────────────────────────────────────────
// Cloudflare Dashboard → R2 → API → Manage API tokens → Create API token
//   權限選 "Object Read & Write"，範圍可只給 msclassic-data。
// 建立後把值寫進專案根目錄的 .env（已被 .gitignore 排除）：
//   R2_ACCOUNT_ID=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
//   R2_ACCESS_KEY_ID=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
//   R2_SECRET_ACCESS_KEY=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
//
// ── 續傳 ──────────────────────────────────────────────────────────────
// out/hosted/.upload/uploaded-<version>.json 記錄每個 key 的「大小:mtime」，
// 中斷後重跑會自動跳過沒變的檔案。--force 可以忽略它。
import { readdir, mkdir, writeFile, readFile, stat } from "node:fs/promises";
import { join, resolve, sep, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { CREDENTIALS_HINT, MIME, encodeKeyPath, getCredentials, loadEnv, signRequest } from "./lib/r2_s3.mjs";

const BUCKET = "msclassic-data";
const DEFAULT_CONCURRENCY = 48;
const MIN_CONCURRENCY = 4;
const MAX_ATTEMPTS = 8;
const REST_CHUNK_SIZE = 1000;
// REST 路徑：1,200 requests / 5 分鐘 = 4 req/s，抓 3.5 留安全邊際。
const REST_RATE_PER_SEC = 3.5;
// REST 路徑撞到 429 之後整個帳號被鎖 5 分鐘，退避一定要 > 300 秒。
const REST_COOLDOWN_MS = 320_000;

// ── 參數 ────────────────────────────────────────────────────────────────
const argv = process.argv.slice(2);
const version = argv.find((a) => !a.startsWith("--"));
const has = (flag) => argv.includes(flag);
const numArg = (flag, fallback) => {
  const i = argv.indexOf(flag);
  if (i === -1) return fallback;
  const n = Number(argv[i + 1]);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : fallback;
};
const dryRun = has("--dry-run");
const force = has("--force");
const forceRest = has("--rest");
const maxConcurrency = numArg("--concurrency", DEFAULT_CONCURRENCY);
const sampleSize = numArg("--sample", 0);

if (!version) {
  console.error(
    "用法：node scripts/upload_hosted.mjs <version> [--dry-run] [--force] [--concurrency N] [--sample N] [--rest]",
  );
  process.exit(1);
}

const ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const DATA_DIR = join(ROOT, "out", "hosted", version);
const UPLOAD_DIR = join(ROOT, "out", "hosted", ".upload");
const MANIFEST_FILE = join(UPLOAD_DIR, `uploaded-${version}.json`);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const fmtBytes = (n) =>
  n >= 1 << 30 ? `${(n / (1 << 30)).toFixed(2)} GB` : `${(n / (1 << 20)).toFixed(1)} MB`;
const fmtDuration = (sec) => {
  if (!Number.isFinite(sec) || sec < 0) return "--";
  const s = Math.round(sec);
  return s >= 60 ? `${Math.floor(s / 60)}m${String(s % 60).padStart(2, "0")}s` : `${s}s`;
};

// ── 掃描檔案 ────────────────────────────────────────────────────────────
/** 遞迴收集目錄下所有檔案（含 size / mtime，供續傳比對）。 */
async function collectFiles(dir) {
  const out = [];
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...(await collectFiles(full)));
    } else if (entry.isFile()) {
      const s = await stat(full);
      out.push({ file: full, size: s.size, stamp: `${s.size}:${Math.floor(s.mtimeMs)}` });
    }
  }
  return out;
}

// ── 主流程 ──────────────────────────────────────────────────────────────
await loadEnv(ROOT);

const all = (await collectFiles(DATA_DIR).catch(() => [])).sort((a, b) => a.file.localeCompare(b.file));
if (all.length === 0) {
  console.error(`[upload] 找不到資料：${DATA_DIR}`);
  process.exit(1);
}
for (const item of all) {
  item.key = `${version}/${item.file.slice(DATA_DIR.length + 1).split(sep).join("/")}`;
}

await mkdir(UPLOAD_DIR, { recursive: true });
let manifest = {};
if (!force) {
  try {
    manifest = JSON.parse(await readFile(MANIFEST_FILE, "utf8"));
  } catch {
    manifest = {};
  }
}

let pending = all.filter((item) => manifest[item.key] !== item.stamp);
const resumeSkipped = all.length - pending.length;
if (sampleSize > 0 && sampleSize < pending.length) {
  // 等距取樣，讓小範圍實測涵蓋不同目錄與副檔名，而不是全擠在字母序最前面那一區。
  const step = pending.length / sampleSize;
  pending = Array.from({ length: sampleSize }, (_, i) => pending[Math.floor(i * step)]);
}
const totalBytes = all.reduce((n, f) => n + f.size, 0);
const pendingBytes = pending.reduce((n, f) => n + f.size, 0);

const credentials = getCredentials();
const useS3 = !forceRest && credentials !== null;

console.log(
  `[upload] ${version}：共 ${all.length} 個檔案 / ${fmtBytes(totalBytes)}，` +
    `待上傳 ${pending.length} 個 / ${fmtBytes(pendingBytes)}` +
    (sampleSize > 0 ? `（--sample ${sampleSize}：等距取樣的實測子集）` : "") +
    (resumeSkipped > 0 ? `（續傳跳過 ${resumeSkipped} 個）` : ""),
);

if (pending.length === 0) {
  console.log("[upload] 沒有需要上傳的檔案。");
  process.exit(0);
}

if (useS3) {
  console.log(`[upload] 走 S3 相容 API（最大並發 ${maxConcurrency}，會依 429/5xx 自動調整）。`);
} else if (forceRest) {
  console.log("[upload] --rest：強制走 wrangler REST 路徑（4 req/s 上限，會很慢）。");
} else {
  console.log(
    `[upload] ${CREDENTIALS_HINT}\n` +
      "[upload] 退回 wrangler REST 路徑：受 1,200 requests / 5 分鐘的帳號限制，" +
      `預估至少 ${fmtDuration(pending.length / REST_RATE_PER_SEC)}。`,
  );
}

if (dryRun) {
  console.log("[upload] --dry-run：不呼叫任何上傳 API。");
  process.exit(0);
}

// ── 續傳紀錄：批次寫回，避免每個檔案都動硬碟 ─────────────────────────────
let manifestDirty = false;
let flushing = false;
async function flushManifest() {
  if (!manifestDirty || flushing) return;
  flushing = true;
  manifestDirty = false;
  await writeFile(MANIFEST_FILE, JSON.stringify(manifest));
  flushing = false;
}
const flushTimer = setInterval(() => void flushManifest(), 3000);
flushTimer.unref?.();

const startedAt = Date.now();
let doneCount = 0;
let doneBytes = 0;
const failures = [];

function reportProgress(finalLine = false) {
  const elapsed = (Date.now() - startedAt) / 1000;
  const rate = doneCount / Math.max(elapsed, 0.001);
  const mbps = doneBytes / (1 << 20) / Math.max(elapsed, 0.001);
  const eta = rate > 0 ? (pending.length - doneCount) / rate : Infinity;
  const line =
    `[upload] ${doneCount}/${pending.length} 檔  ${fmtBytes(doneBytes)}  ` +
    `${rate.toFixed(1)} 檔/s  ${mbps.toFixed(1)} MB/s  已用 ${fmtDuration(elapsed)}  剩 ${fmtDuration(eta)}`;
  process.stdout.write(`\r${line.padEnd(100)}${finalLine ? "\n" : ""}`);
}
const progressTimer = setInterval(() => reportProgress(), 1000);
progressTimer.unref?.();

if (useS3) {
  await uploadViaS3();
} else {
  await uploadViaWranglerRest();
}

clearInterval(progressTimer);
clearInterval(flushTimer);
manifestDirty = true;
await flushManifest();
reportProgress(true);

if (failures.length > 0) {
  console.error(`[upload] ${failures.length} 個檔案上傳失敗：`);
  for (const f of failures.slice(0, 20)) console.error(`  ${f.key} — ${f.reason}`);
  if (failures.length > 20) console.error(`  …還有 ${failures.length - 20} 個`);
  console.error("[upload] 直接重跑本腳本即可，已完成的會自動跳過。");
  process.exit(1);
}
console.log(`[upload] 全部上傳完成（${fmtDuration((Date.now() - startedAt) / 1000)}）。`);

// ── S3 相容 API 上傳 ────────────────────────────────────────────────────
async function uploadViaS3() {
  const { host } = credentials;

  // 自適應並發：撞到 429/5xx 就折半並全體冷卻，之後隨成功次數慢慢加回來。
  let limit = Math.min(maxConcurrency, 16);
  let inFlight = 0;
  let pauseUntil = 0;
  let sinceThrottle = 0;
  const waiters = [];

  async function acquire() {
    for (;;) {
      const wait = pauseUntil - Date.now();
      if (wait > 0) {
        await sleep(Math.min(wait, 250));
        continue;
      }
      if (inFlight < limit) {
        inFlight++;
        return;
      }
      await new Promise((r) => waiters.push(r));
    }
  }
  function release() {
    inFlight--;
    waiters.shift()?.();
  }
  function onThrottled(attempt) {
    limit = Math.max(MIN_CONCURRENCY, Math.floor(limit / 2));
    sinceThrottle = 0;
    pauseUntil = Math.max(pauseUntil, Date.now() + Math.min(30_000, 1000 * 2 ** attempt));
  }
  function onSuccess() {
    if (++sinceThrottle >= 50 && limit < maxConcurrency) {
      limit++;
      sinceThrottle = 0;
      waiters.shift()?.();
    }
  }

  async function putOne(item) {
    const body = await readFile(item.file);
    const canonicalPath = `/${BUCKET}/${encodeKeyPath(item.key)}`;
    const contentType = MIME[extname(item.key).toLowerCase()] ?? "application/octet-stream";
    let lastReason = "unknown";

    for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
      await acquire();
      try {
        const headers = signRequest({
          method: "PUT",
          host,
          canonicalPath,
          headers: { "content-type": contentType },
          body,
          credentials,
        });
        const res = await fetch(`https://${host}${canonicalPath}`, { method: "PUT", headers, body });
        if (res.ok) {
          onSuccess();
          return true;
        }
        lastReason = `HTTP ${res.status}`;
        // 429 = 速率限制，5xx = R2 過載，兩者都退避重試；4xx 其他多半是設定錯誤。
        if (res.status !== 429 && res.status < 500) {
          const detail = (await res.text().catch(() => "")).slice(0, 200);
          lastReason = `HTTP ${res.status} ${detail}`;
          break;
        }
        onThrottled(attempt);
      } catch (err) {
        lastReason = String(err?.message ?? err);
        onThrottled(attempt);
      } finally {
        release();
      }
      await sleep(Math.min(15_000, 300 * 2 ** attempt) * (0.5 + Math.random()));
    }
    failures.push({ key: item.key, reason: lastReason });
    return false;
  }

  let cursor = 0;
  const workerCount = Math.min(maxConcurrency, pending.length);
  await Promise.all(
    Array.from({ length: workerCount }, async () => {
      for (;;) {
        const i = cursor++;
        if (i >= pending.length) return;
        const item = pending[i];
        if (await putOne(item)) {
          manifest[item.key] = item.stamp;
          manifestDirty = true;
          doneBytes += item.size;
        }
        doneCount++;
      }
    }),
  );
}

// ── wrangler REST 備援：刻意壓在 4 req/s 以下，並且 429 後冷卻 5 分鐘 ────
async function uploadViaWranglerRest() {
  const wranglerJs = join(ROOT, "node_modules", "wrangler", "bin", "wrangler.js");

  /** 執行一批 bulk put；成功回 true。 */
  function putChunk(chunkPath) {
    return new Promise((resolvePromise) => {
      // 直接跑 node_modules 裡的 wrangler.js：Windows 上 spawn npx.cmd 會被
      // Node 的 batch 檔保護擋下（spawn EINVAL），比照 dev_hosted.mjs 對 vite 的做法。
      const child = spawn(
        process.execPath,
        [wranglerJs, "r2", "bulk", "put", BUCKET, "--remote", "-y", "-f", chunkPath, "--concurrency", "4"],
        { stdio: ["ignore", "ignore", "inherit"] },
      );
      child.on("error", () => resolvePromise(false));
      child.on("close", (code) => resolvePromise(code === 0));
    });
  }

  for (let start = 0; start < pending.length; start += REST_CHUNK_SIZE) {
    const chunkStartedAt = Date.now();
    const chunk = pending.slice(start, start + REST_CHUNK_SIZE);
    const chunkPath = join(UPLOAD_DIR, `chunk-${String(start / REST_CHUNK_SIZE).padStart(3, "0")}.json`);
    await writeFile(chunkPath, JSON.stringify(chunk.map(({ key, file }) => ({ key, file }))));

    let ok = false;
    for (let attempt = 1; attempt <= 4 && !ok; attempt++) {
      if (attempt > 1) {
        process.stdout.write(`\n[upload] 批次失敗，等 ${REST_COOLDOWN_MS / 1000}s 讓帳號速率限制解除…\n`);
        await sleep(REST_COOLDOWN_MS);
      }
      ok = await putChunk(chunkPath);
    }

    if (ok) {
      for (const item of chunk) {
        manifest[item.key] = item.stamp;
        doneBytes += item.size;
      }
      manifestDirty = true;
      await flushManifest();
    } else {
      for (const item of chunk) failures.push({ key: item.key, reason: "wrangler bulk put 失敗" });
    }
    doneCount += chunk.length;

    // 主動配速：本批花的時間若少於 REST_RATE_PER_SEC 應有的時間就補睡，
    // 確保平均請求速率低於 4 req/s，不會觸發 5 分鐘全帳號封鎖。
    const budgetMs = (chunk.length / REST_RATE_PER_SEC) * 1000;
    const spentMs = Date.now() - chunkStartedAt;
    if (spentMs < budgetMs) await sleep(budgetMs - spentMs);
  }
}

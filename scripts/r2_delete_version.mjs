// 刪除 R2 上某個舊版本的整包資料集（bucket: msclassic-data）。
//
// 用法：node scripts/r2_delete_version.mjs <version> [--yes] [--force]
//   例：node scripts/r2_delete_version.mjs v20260804          # 只列出會刪什麼，不動手
//       node scripts/r2_delete_version.mjs v20260804 --yes    # 真的刪
//
// 選項：
//   --yes    真的執行刪除（不加只做 dry-run 統計，這是預設）。
//   --force  忽略「這個版本正在被前端使用」的保護（危險，正常不該用到）。
//
// 走 S3 相容 API 的 DeleteObjects：一次最多 1,000 個 key，三萬個物件只要 30 個請求。
// 用 wrangler r2 object delete 一個個刪會走 Cloudflare REST API，
// 撞上 1,200 requests / 5 分鐘的帳號限制，三萬個檔案要跑兩小時以上。
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CREDENTIALS_HINT,
  getCredentials,
  listKeys,
  loadEnv,
  md5base64,
  signRequest,
  unescapeXml,
} from "./lib/r2_s3.mjs";

const BUCKET = "msclassic-data";
const BATCH = 1000;

const argv = process.argv.slice(2);
const version = argv.find((a) => !a.startsWith("--"));
const confirmed = argv.includes("--yes");
const force = argv.includes("--force");

if (!version || !/^v\d{8}$/.test(version)) {
  console.error("用法：node scripts/r2_delete_version.mjs <version> [--yes] [--force]");
  console.error("      <version> 必須是 vYYYYMMDD 格式，例如 v20260804。");
  process.exit(1);
}

const ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
await loadEnv(ROOT);

const credentials = getCredentials();
if (!credentials) {
  console.error(CREDENTIALS_HINT);
  process.exit(1);
}

// 保護：不准刪掉前端目前指向的版本（刪了線上站會整個 404）。
const viteConfig = await readFile(resolve(ROOT, "vite.config.hosted.ts"), "utf8");
const inUse = /VITE_HOSTED_DATA_VERSION\s*\?\?\s*"(v\d{8})"/.exec(viteConfig)?.[1];
if (inUse === version && !force) {
  console.error(
    `[delete] 拒絕執行：vite.config.hosted.ts 目前指向的就是 ${version}，` +
      "刪掉線上站的資料會全部 404。\n" +
      "         請先把版本切到新的資料集、部署並確認正常，再回來刪這個。",
  );
  process.exit(1);
}
if (inUse) console.log(`[delete] 前端目前使用的版本：${inUse}（要刪的是 ${version}）`);

const { host } = credentials;
const prefix = `${version}/`;

console.log(`[delete] 掃描 r2://${BUCKET}/${prefix} …`);
const batches = [];
let total = 0;
for await (const keys of listKeys({ host, bucket: BUCKET, prefix, credentials })) {
  batches.push(keys);
  total += keys.length;
  process.stdout.write(`\r[delete] 已列出 ${total} 個物件…`);
}
process.stdout.write("\n");

if (total === 0) {
  console.log(`[delete] ${prefix} 底下沒有任何物件，不需要處理。`);
  process.exit(0);
}

console.log(`[delete] 共 ${total} 個物件，會用 ${Math.ceil(total / BATCH)} 個 DeleteObjects 請求刪除。`);
console.log(`[delete] 抽樣：${batches[0].slice(0, 3).join("\n           ")}`);

if (!confirmed) {
  console.log("[delete] 這是 dry-run（沒有加 --yes），什麼都沒刪。確認無誤後加 --yes 再跑一次。");
  process.exit(0);
}

let deleted = 0;
const errors = [];
for (const batch of batches) {
  for (let i = 0; i < batch.length; i += BATCH) {
    const slice = batch.slice(i, i + BATCH);
    const xml =
      `<?xml version="1.0" encoding="UTF-8"?><Delete><Quiet>true</Quiet>` +
      slice
        .map(
          (k) =>
            `<Object><Key>${k.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")}</Key></Object>`,
        )
        .join("") +
      `</Delete>`;
    const body = Buffer.from(xml, "utf8");
    const canonicalPath = `/${BUCKET}`;
    const canonicalQuery = "delete=";
    const headers = signRequest({
      method: "POST",
      host,
      canonicalPath,
      canonicalQuery,
      headers: { "content-md5": md5base64(body), "content-type": "application/xml" },
      body,
      credentials,
    });
    const res = await fetch(`https://${host}${canonicalPath}?${canonicalQuery}`, {
      method: "POST",
      headers,
      body,
    });
    const text = await res.text();
    if (!res.ok) {
      errors.push(`HTTP ${res.status}：${text.slice(0, 300)}`);
      continue;
    }
    // Quiet 模式只回傳失敗的項目。
    for (const m of text.matchAll(/<Error>[\s\S]*?<Key>([\s\S]*?)<\/Key>[\s\S]*?<Code>([\s\S]*?)<\/Code>/g)) {
      errors.push(`${unescapeXml(m[1])} — ${unescapeXml(m[2])}`);
    }
    deleted += slice.length;
    process.stdout.write(`\r[delete] 已刪除 ${deleted}/${total} …`);
  }
}
process.stdout.write("\n");

if (errors.length > 0) {
  console.error(`[delete] ${errors.length} 個項目回報失敗：`);
  for (const e of errors.slice(0, 20)) console.error(`  ${e}`);
  if (errors.length > 20) console.error(`  …還有 ${errors.length - 20} 個`);
  console.error("[delete] 重跑本腳本會重新掃描，只處理還存在的物件。");
  process.exit(1);
}
console.log(`[delete] ${version} 已從 R2 移除（${deleted} 個物件）。`);

// R2 S3 相容 API 的共用工具：讀 .env、AWS SigV4 簽章、路徑編碼。
//
// 為什麼不走 Cloudflare REST API（api.cloudflare.com）：
//   那個介面全帳號硬上限 1,200 requests / 5 分鐘（≈ 4 req/s），超過之後接下來
//   5 分鐘所有 API 呼叫都被擋。三萬個物件的上傳／刪除完全跑不動。
//   S3 相容 API 沒有帳號層級的請求數上限，只有「同一個 key 每秒 1 次寫入」。
//   https://developers.cloudflare.com/r2/platform/limits/#cloudflare-rest-api
//
// 簽章實作已用 AWS 官方 PUT Object 參考向量驗證過（見 git log）。
import { readFile } from "node:fs/promises";
import { createHash, createHmac } from "node:crypto";
import { join } from "node:path";

export const MIME = {
  ".json": "application/json; charset=utf-8",
  ".gz": "application/gzip",
  ".webp": "image/webp",
  ".ogg": "audio/ogg",
  ".png": "image/png",
};

/** 讀專案根目錄的 .env（只補沒設定過的環境變數，不覆蓋既有的）。 */
export async function loadEnv(root) {
  let text;
  try {
    text = await readFile(join(root, ".env"), "utf8");
  } catch {
    return;
  }
  for (const line of text.split(/\r?\n/)) {
    const m = /^\s*(?:export\s+)?([A-Z0-9_]+)\s*=\s*(.*)$/.exec(line);
    if (!m) continue;
    const value = m[2].trim().replace(/^(['"])(.*)\1$/, "$2");
    if (process.env[m[1]] === undefined) process.env[m[1]] = value;
  }
}

/** 取得 R2 憑證；缺任何一項回 null。 */
export function getCredentials() {
  const accountId = process.env.R2_ACCOUNT_ID || process.env.CLOUDFLARE_ACCOUNT_ID;
  const accessKeyId = process.env.R2_ACCESS_KEY_ID;
  const secretAccessKey = process.env.R2_SECRET_ACCESS_KEY;
  if (!accountId || !accessKeyId || !secretAccessKey) return null;
  return { accountId, accessKeyId, secretAccessKey, host: `${accountId}.r2.cloudflarestorage.com` };
}

export const CREDENTIALS_HINT =
  "找不到 R2 API token（R2_ACCOUNT_ID / R2_ACCESS_KEY_ID / R2_SECRET_ACCESS_KEY）。\n" +
  "取得方式：Cloudflare Dashboard → R2 → API → Manage API tokens → Create API token，\n" +
  "權限選 Object Read & Write，把值填進專案根目錄 .env（格式見 .env.example）。";

export const sha256hex = (buf) => createHash("sha256").update(buf).digest("hex");
export const md5base64 = (buf) => createHash("md5").update(buf).digest("base64");
const hmac = (key, data) => createHmac("sha256", key).update(data).digest();

/** RFC3986 逐段編碼；`/` 保留成路徑分隔。 */
export const encodeKeyPath = (key) =>
  key
    .split("/")
    .map((seg) =>
      encodeURIComponent(seg).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`),
    )
    .join("/");

/**
 * 產生 SigV4 簽章 headers。
 * @param canonicalPath 已編碼的路徑，如 `/msclassic-data/v1/a%20b.webp`
 * @param canonicalQuery 已編碼且**依 key 排序**的 query string，如 `list-type=2&prefix=v1%2F`
 * @param headers 除了 host / x-amz-date / x-amz-content-sha256 以外要一起簽的 header（key 用小寫）
 */
export function signRequest({ method, host, canonicalPath, canonicalQuery = "", headers = {}, body, credentials }) {
  const amzDate = new Date().toISOString().replace(/[:-]|\.\d{3}/g, "");
  const dateStamp = amzDate.slice(0, 8);
  const payloadHash = sha256hex(body ?? Buffer.alloc(0));
  const signed = {
    ...headers,
    host,
    "x-amz-content-sha256": payloadHash,
    "x-amz-date": amzDate,
  };
  const names = Object.keys(signed).sort();
  const signedHeaders = names.join(";");
  const canonicalHeaders = names.map((h) => `${h}:${signed[h]}\n`).join("");
  const canonicalRequest = [
    method,
    canonicalPath,
    canonicalQuery,
    canonicalHeaders,
    signedHeaders,
    payloadHash,
  ].join("\n");
  const scope = `${dateStamp}/auto/s3/aws4_request`;
  const stringToSign = [
    "AWS4-HMAC-SHA256",
    amzDate,
    scope,
    sha256hex(Buffer.from(canonicalRequest, "utf8")),
  ].join("\n");
  let k = hmac(`AWS4${credentials.secretAccessKey}`, dateStamp);
  k = hmac(k, "auto");
  k = hmac(k, "s3");
  k = hmac(k, "aws4_request");
  const signature = createHmac("sha256", k).update(stringToSign).digest("hex");
  return {
    ...signed,
    authorization:
      `AWS4-HMAC-SHA256 Credential=${credentials.accessKeyId}/${scope}, ` +
      `SignedHeaders=${signedHeaders}, Signature=${signature}`,
  };
}

/** XML 文字節點反跳脫（R2 的 ListObjectsV2 回應會跳脫 & < > 等字元）。 */
export const unescapeXml = (s) =>
  s
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&amp;/g, "&");

/** 列出某個 prefix 底下的所有 key（ListObjectsV2，每次 1,000 筆）。 */
export async function* listKeys({ host, bucket, prefix, credentials }) {
  let token;
  do {
    const params = [
      ["list-type", "2"],
      ["max-keys", "1000"],
      ["prefix", prefix],
      ...(token ? [["continuation-token", token]] : []),
    ].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    const canonicalQuery = params.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join("&");
    const canonicalPath = `/${bucket}`;
    const headers = signRequest({ method: "GET", host, canonicalPath, canonicalQuery, credentials });
    const res = await fetch(`https://${host}${canonicalPath}?${canonicalQuery}`, { headers });
    const xml = await res.text();
    if (!res.ok) throw new Error(`ListObjectsV2 失敗 HTTP ${res.status}：${xml.slice(0, 300)}`);

    const keys = [...xml.matchAll(/<Key>([\s\S]*?)<\/Key>/g)].map((m) => unescapeXml(m[1]));
    if (keys.length > 0) yield keys;

    const truncated = /<IsTruncated>\s*true\s*<\/IsTruncated>/.test(xml);
    const next = /<NextContinuationToken>([\s\S]*?)<\/NextContinuationToken>/.exec(xml);
    token = truncated && next ? unescapeXml(next[1]) : undefined;
  } while (token);
}

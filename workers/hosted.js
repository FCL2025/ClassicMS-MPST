// Hosted 模式 Worker：靜態站（ASSETS = dist-hosted）+ 版本化資料集（R2 binding DATA）。
//
// 路由：
//   GET /data/<version>/<path...> → R2 key `<version>/<path...>`
//     - 路徑帶版本，內容不可變 → Cache-Control: public, max-age=31536000, immutable
//     - 全部 /data 回應帶 Access-Control-Allow-Origin: *（dev 時前端跑在別的 port）
//     - .json → 實際存的是 <key>.gz（gzip 壓縮），回 Content-Type: application/json
//       + Content-Encoding: gzip，瀏覽器透明解壓；若 .gz 不存在退回未壓縮的 key
//       （相容過渡期未壓縮的舊資料）。
//   其他路徑 → env.ASSETS.fetch(request)（hash 路由，不需要 SPA fallback）
//
// 無依賴、plain JS（Service Worker 格式），設定見 wrangler.hosted.jsonc。

const DATA_PREFIX = "/data/";
const IMMUTABLE_CACHE = "public, max-age=31536000, immutable";

const MIME = {
  ".json": "application/json; charset=utf-8",
  ".webp": "image/webp",
  ".ogg": "audio/ogg",
};

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname.startsWith(DATA_PREFIX)) {
      return handleData(request, env, url);
    }
    return env.ASSETS.fetch(request);
  },
};

async function handleData(request, env, url) {
  const cors = { "Access-Control-Allow-Origin": "*" };
  if (request.method === "OPTIONS") {
    return new Response(null, { status: 204, headers: cors });
  }
  if (request.method !== "GET" && request.method !== "HEAD") {
    return new Response("Method Not Allowed", { status: 405, headers: cors });
  }

  const key = decodeURIComponent(url.pathname.slice(DATA_PREFIX.length));
  if (!key || key.includes("..")) {
    return new Response("Not Found", { status: 404, headers: cors });
  }

  const ext = key.slice(key.lastIndexOf(".")).toLowerCase();
  const headers = {
    ...cors,
    "Content-Type": MIME[ext] ?? "application/octet-stream",
    "Cache-Control": IMMUTABLE_CACHE,
  };

  let object;
  if (ext === ".json") {
    // JSON 產物以 gzip 存放（key + ".gz"）；取不到時退回未壓縮的 key。
    object = await env.DATA.get(`${key}.gz`);
    if (object) {
      headers["Content-Encoding"] = "gzip";
    } else {
      object = await env.DATA.get(key);
    }
  } else {
    object = await env.DATA.get(key);
  }
  if (!object) {
    return new Response("Not Found", { status: 404, headers: cors });
  }

  headers["Content-Length"] = String(object.size);
  if (request.method === "HEAD") {
    return new Response(null, { status: 200, headers });
  }
  return new Response(object.body, { status: 200, headers });
}

// Hosted 資料集的本機靜態伺服器（開發用；線上由 R2/Workers 取代）。
// 零依賴：serve out/hosted/，port 8788（可用環境變數 HOSTED_PORT 覆寫），
// 全部回應帶 Access-Control-Allow-Origin: * 讓 vite dev server（1422）跨域抓取。
// 請求 *.json 而磁碟上只有 *.json.gz 時，回 gzip 內容 + Content-Encoding: gzip
// （與 workers/hosted.js 行為一致）。
import { createServer } from "node:http";
import { stat } from "node:fs/promises";
import { createReadStream } from "node:fs";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(fileURLToPath(new URL("../out/hosted", import.meta.url)));
const PORT = Number(process.env.HOSTED_PORT ?? 8788);

const MIME = {
  ".json": "application/json; charset=utf-8",
  ".webp": "image/webp",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".ogg": "audio/ogg",
  ".mp3": "audio/mpeg",
  ".txt": "text/plain; charset=utf-8",
};

const server = createServer(async (req, res) => {
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "*");
  if (req.method === "OPTIONS") {
    res.writeHead(204);
    res.end();
    return;
  }
  if (req.method !== "GET" && req.method !== "HEAD") {
    res.writeHead(405);
    res.end("Method Not Allowed");
    return;
  }
  try {
    const url = new URL(req.url ?? "/", "http://localhost");
    const pathname = decodeURIComponent(url.pathname);
    // 防目錄穿越：resolve 後必須仍在 ROOT 內。
    const filePath = resolve(join(ROOT, pathname));
    if (filePath !== ROOT && !filePath.startsWith(ROOT + sep)) {
      res.writeHead(404);
      res.end("Not Found");
      return;
    }
    let target = filePath;
    let gzipped = false;
    let info = await stat(target).catch(() => null);
    // JSON 產物即將全面改成 <name>.json.gz（gzip 壓縮）：請求 .json 而磁碟上
    // 只有 .json.gz 時，回 .gz 內容 + Content-Encoding: gzip，瀏覽器透明解壓。
    // 兩者並存時優先未壓縮檔（相容過渡期）。
    if ((!info || !info.isFile()) && filePath.endsWith(".json")) {
      const gzPath = `${filePath}.gz`;
      const gzInfo = await stat(gzPath).catch(() => null);
      if (gzInfo?.isFile()) {
        target = gzPath;
        info = gzInfo;
        gzipped = true;
      }
    }
    if (!info || !info.isFile()) {
      res.writeHead(404);
      res.end("Not Found");
      return;
    }
    const headers = {
      "Content-Type": MIME[extname(filePath).toLowerCase()] ?? "application/octet-stream",
      "Content-Length": info.size,
      "Cache-Control": "no-cache",
    };
    if (gzipped) {
      headers["Content-Encoding"] = "gzip";
      headers["Vary"] = "Accept-Encoding";
    }
    res.writeHead(200, headers);
    if (req.method === "HEAD") {
      res.end();
      return;
    }
    createReadStream(target).pipe(res);
  } catch {
    res.writeHead(404);
    res.end("Not Found");
  }
});

server.listen(PORT, () => {
  console.log(`[hosted] 資料根目錄：${ROOT}`);
  console.log(`[hosted] 靜態伺服器已啟動：http://localhost:${PORT}`);
});

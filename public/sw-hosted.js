// Hosted 模式的 Service Worker：對同源 /data/ 的 GET 做 cache-first 快取。
// 資料集路徑含版本（/data/<version>/...）、內容不可變，命中即回、未命中抓回存入。
// activate 時清掉同前缀的舊 cache key。
//
// 本檔放在 public/，桌面版與 web 版 build 也會原樣複製，但只有 hosted 版
// （src/api.hosted.ts）會註冊它，其他版本完全不影響，屬無害的多餘檔案。

// v2 清掉修正預壓縮回應前可能留下的雙層 gzip 快取。
const CACHE_NAME = "msclassic-hosted-data-v2";
const CACHE_PREFIX = "msclassic-hosted";
const DATA_PREFIX = "/data/";

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          .filter((key) => key.startsWith(CACHE_PREFIX) && key !== CACHE_NAME)
          .map((key) => caches.delete(key)),
      );
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (!url.pathname.startsWith(DATA_PREFIX)) return;

  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE_NAME);
      const hit = await cache.match(request);
      if (hit) return hit;
      const response = await fetch(request);
      if (response.ok) {
        event.waitUntil(cache.put(request, response.clone()));
      }
      return response;
    })(),
  );
});

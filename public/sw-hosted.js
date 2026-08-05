// Hosted 模式舊 Service Worker 的退役腳本。
// 先前的 cache-first 策略會永久保留曾經損壞的 JSON；新版前端不再註冊
// Service Worker，留下此檔只為讓已安裝的舊版本更新後清掉快取並自行註銷。
const CACHE_PREFIX = "msclassic-hosted";

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          .filter((key) => key.startsWith(CACHE_PREFIX))
          .map((key) => caches.delete(key)),
      );
      await self.clients.claim();
      await self.registration.unregister();
    })(),
  );
});

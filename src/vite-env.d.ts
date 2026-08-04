/// <reference types="vite/client" />

/** 建置期常數:web(WASM)版為 true,桌面(Tauri)版為 false。 */
declare const __WEB_BUILD__: boolean;

/** hosted 版建置期常數:靜態資料集根 URL（vite define 注入，見 vite.config.hosted.ts）。 */
declare const __HOSTED_DATA_BASE__: string;

/** hosted 版建置期常數:資料集版本（URL 前綴，如 "v20260804"）。 */
declare const __HOSTED_DATA_VERSION__: string;

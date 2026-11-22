// 經典楓之谷視窗邊框：從遊戲素材 `UI/UIWindow/Quest/backgrnd`（乾淨的標題列
// + 白底內容 + 底邊）裁出九宮格來源圖，給 CSS border-image 用。
// 載入失敗（無遊戲資料、web 版尚未載入資料集等）回傳 null，呼叫端用備援樣式。

import { getImage } from "../api";
import { getActiveLocale, onDatasetChange } from "../locale";

// UI 素材的語系目錄隨資料集而定（TW/CN/JP/KR…），路徑在載入時才組出。
const framePath = () => `${getActiveLocale()}/UI/UIWindow/Quest/backgrnd`;

// 來源圖 245x396：標題列 y0..17（左上角有內建「任務」二字，取右側乾淨漸層）、
// 內容白底（左右邊框各約 7px）、底部鋼藍橫條 y369..396。
const SRC_W = 245;
const CORNER = 7;
const TITLE_H = 17;
const MID_Y = 100;
const MID_H = 30;
const FOOTER_Y = 369;
const FOOTER_H = 27;
const STRIP_X = 150; // 乾淨（無文字）的中段取樣起點
const STRIP_W = 30;

// 載入結果快取（同一語系只需組一次）；null 表示取得失敗。切換語系時作廢。
let cached: Promise<string | null> | undefined;
onDatasetChange(() => {
  cached = undefined;
});

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = reject;
    img.src = src;
  });
}

export function loadClassicFrame(): Promise<string | null> {
  return (cached ??= buildClassicFrame());
}

async function buildClassicFrame(): Promise<string | null> {
  try {
    const result = await getImage(framePath());
    if (!result) return null;
    const img = await loadImage(`data:image/png;base64,${result.base64}`);

    const w = CORNER + STRIP_W + CORNER;
    const h = TITLE_H + MID_H + FOOTER_H;
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;

    const right = SRC_W - CORNER;
    const midOut = TITLE_H;
    const botOut = TITLE_H + MID_H;
    // 標題列：左角、乾淨中段、右角
    ctx.drawImage(img, 0, 0, CORNER, TITLE_H, 0, 0, CORNER, TITLE_H);
    ctx.drawImage(img, STRIP_X, 0, STRIP_W, TITLE_H, CORNER, 0, STRIP_W, TITLE_H);
    ctx.drawImage(img, right, 0, CORNER, TITLE_H, CORNER + STRIP_W, 0, CORNER, TITLE_H);
    // 內容：左邊、白底、右邊
    ctx.drawImage(img, 0, MID_Y, CORNER, MID_H, 0, midOut, CORNER, MID_H);
    ctx.drawImage(img, 10, MID_Y, STRIP_W, MID_H, CORNER, midOut, STRIP_W, MID_H);
    ctx.drawImage(img, right, MID_Y, CORNER, MID_H, CORNER + STRIP_W, midOut, CORNER, MID_H);
    // 底邊：左角、中段、右角
    ctx.drawImage(img, 0, FOOTER_Y, CORNER, FOOTER_H, 0, botOut, CORNER, FOOTER_H);
    ctx.drawImage(img, STRIP_X, FOOTER_Y, STRIP_W, FOOTER_H, CORNER, botOut, STRIP_W, FOOTER_H);
    ctx.drawImage(img, right, FOOTER_Y, CORNER, FOOTER_H, CORNER + STRIP_W, botOut, CORNER, FOOTER_H);

    return canvas.toDataURL("image/png");
  } catch {
    return null;
  }
}

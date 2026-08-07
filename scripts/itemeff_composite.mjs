// ItemEff 驗證合成器：讀 out/itemeff/render/scene.json + 圖層 PNG，
// 以 body 幀 0 + face 幀 0 + 指定 effect 幀合成 3x 預覽圖（z 遞減=底層先畫）。
// 用法：node scripts/itemeff_composite.mjs [effect幀序，逗號分隔] （預設 0,4,8）

import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import sharp from "sharp";

const dir = "out/itemeff/render";
const scene = JSON.parse(await readFile(path.join(dir, "scene.json"), "utf8"));
const SCALE = 3;

const imgFile = (p) => path.join(dir, p.replaceAll("/", "__") + ".png");

// face 相對座標 → 用 body 幀 0 的 faceAnchor 配對（對齊前端 composeLayers）
const body0 = scene.frames[0];
const face0 = scene.faceFrames[0];

async function compose(effectIndex, outName) {
  const layers = [...body0.layers];
  if (body0.showFace && body0.faceAnchor && face0) {
    for (const l of face0.layers) {
      layers.push({ ...l, x: body0.faceAnchor[0] + l.x, y: body0.faceAnchor[1] + l.y });
    }
  }
  const eff = scene.effectFrames[effectIndex];
  if (eff) layers.push(...eff.layers);
  layers.sort((a, b) => b.z - a.z);

  // 量每張圖尺寸算 bbox
  const metas = new Map();
  for (const l of layers) {
    if (!metas.has(l.path)) metas.set(l.path, await sharp(imgFile(l.path)).metadata());
  }
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const l of layers) {
    const m = metas.get(l.path);
    minX = Math.min(minX, l.x); minY = Math.min(minY, l.y);
    maxX = Math.max(maxX, l.x + m.width); maxY = Math.max(maxY, l.y + m.height);
  }
  const width = Math.ceil(maxX - minX) * SCALE;
  const height = Math.ceil(maxY - minY) * SCALE;
  const composites = [];
  for (const l of layers) {
    const buf = await sharp(imgFile(l.path))
      .resize(metas.get(l.path).width * SCALE, metas.get(l.path).height * SCALE, { kernel: "nearest" })
      .toBuffer();
    composites.push({
      input: buf,
      left: Math.round((l.x - minX) * SCALE),
      top: Math.round((l.y - minY) * SCALE),
    });
  }
  // 棋盤格背景方便看透明
  const bg = await sharp({
    create: {
      width, height, channels: 4,
      background: { r: 48, g: 48, b: 48, alpha: 1 },
    },
  }).png().toBuffer();
  await sharp(bg).composite(composites).png().toFile(path.join(dir, outName));
  console.log(`OK ${outName} (${width}x${height}) effect幀=${effectIndex} layers=${layers.length}`);
}

const indices = (process.argv[2] ?? "0,4,8").split(",").map(Number);
for (const i of indices) {
  await compose(i, `cape_check_eff${i}.png`);
}

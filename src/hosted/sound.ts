// BGM → 地圖反查索引（hosted 版）：語意對齊 src-tauri/src/sound.rs。
// 清單直接讀 pipeline 預處理的 features/map_bgm.json。

import { DATA_ROOT, fetchChecked } from "./core";
import type { BgmMapGroup } from "./backend";

let mapBgmPromise: Promise<BgmMapGroup[]> | null = null;

export function getMapBgmIndex(): Promise<BgmMapGroup[]> {
  if (!mapBgmPromise) {
    mapBgmPromise = fetchChecked(`${DATA_ROOT}/features/map_bgm.json`).then(
      (response) => response.json() as Promise<BgmMapGroup[]>,
    );
    mapBgmPromise.catch(() => {
      mapBgmPromise = null;
    });
  }
  return mapBgmPromise;
}

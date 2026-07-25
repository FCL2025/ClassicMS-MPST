//! Phase 5 驗收：匯出子樹 → JSON + PNG，印出統計。
//! 用法：`cargo run --release --example export_demo -- [path] [out_dir]`
//! 預設匯出 `TamingMob/0004`（含 info/speed 等數值與 canvas 圖）到 `out/export`。

use std::path::PathBuf;

use msclassic::catalog::DataService;

const DEFAULT_DATA_DIR: &str =
    r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w";

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "TamingMob/0004".into());
    let out = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("out/export"));
    let cache = PathBuf::from("out/export_catalog_cache.json");
    let (svc, info) = DataService::open(std::path::Path::new(DEFAULT_DATA_DIR), &cache)
        .expect("開啟資料目錄失敗");
    eprintln!(
        "索引 {} 筆（{}，{}ms）",
        info.entry_count,
        if info.from_cache { "快取" } else { "重建" },
        info.elapsed_ms
    );
    let stats = svc.export_subtree(&path, &out).expect("匯出失敗");
    println!(
        "匯出完成：{} → {}\n  JSON {} 個 / PNG {} 個 / 節點 {} 個",
        path, stats.out_dir, stats.json_files, stats.png_files, stats.nodes
    );
}

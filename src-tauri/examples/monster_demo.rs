//! 怪物圖鑑驗收：印清單筆數，並抽查一隻怪物的明細（掉落/動畫幀/地圖）。
//! 用法：`cargo run --release --example monster_demo -- [mob_id]`（預設 2230102 緞帶肥肥）。

use std::path::PathBuf;

use msclassic::catalog::DataService;

const DEFAULT_DATA_DIR: &str =
    r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w";

fn main() {
    let mob_id: u32 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(2230102);
    let cache = PathBuf::from("out/export_catalog_cache.json");
    let (svc, info) = DataService::open(std::path::Path::new(DEFAULT_DATA_DIR), &cache)
        .expect("開啟資料目錄失敗");
    eprintln!(
        "索引 {} 筆（{}，{}ms）",
        info.entry_count,
        if info.from_cache { "快取" } else { "重建" },
        info.elapsed_ms
    );

    let book = svc.get_monster_book().expect("圖鑑清單失敗");
    let with_drops = book.iter().filter(|entry| !entry.drops.is_empty()).count();
    let named = book.iter().filter(|entry| entry.name.is_some()).count();
    println!(
        "圖鑑：{} 隻（有名稱 {named}、有掉落 {with_drops}）",
        book.len()
    );

    let detail = svc.get_mob_detail(mob_id).expect("怪物明細失敗");
    println!(
        "{mob_id} {:?}：Lv.{:?} / 型態 {:?} / HP {} / EXP {}",
        detail.name, detail.level, detail.category, detail.max_hp, detail.exp
    );
    println!(
        "  掉落 {} 項（有 icon {}）、地圖 {} 張、動畫 {} 幀",
        detail.drops.len(),
        detail
            .drops
            .iter()
            .filter(|d| d.icon_base64.is_some())
            .count(),
        detail.maps.len(),
        detail.frames.len()
    );
    for drop in detail.drops.iter().take(6) {
        println!("    {} {:?}", drop.item_id, drop.name);
    }
    if let Some(frame) = detail.frames.first() {
        println!(
            "  首幀 {}x{} origin {:?} delay {}",
            frame.width, frame.height, frame.origin, frame.delay
        );
    }
}

//! Phase 4 取圖驗證：建索引（含圖片索引）→ get_image 取幾個代表路徑存 PNG。
//!
//! 用法：
//! ```text
//! cargo run --release --example img_demo -- [bundle 目錄]
//! ```

use std::path::PathBuf;

use msclassic::catalog::DataService;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w"));
    let requested_path = std::env::args().nth(2);
    let cache = std::env::temp_dir().join("msclassic_catalog_img.json");
    std::fs::remove_file(&cache).ok(); // 圖片索引格式改了，強制重建

    let t0 = std::time::Instant::now();
    let (svc, info) = DataService::open(&root, &cache)?;
    println!(
        "開啟（含圖片索引重建）: {}ms entries={} sheets={} set_assets={}",
        info.elapsed_ms,
        info.entry_count,
        svc.sheets().count(),
        svc.catalog.image.set_assets.len()
    );
    println!("建索引總耗時: {:.2?}", t0.elapsed());

    let default_cases = [
        "Character/Afterimage/blank/axe",         // per-canvas spritesheet
        "Character/Afterimage/blank/swordOL",     // per-canvas spritesheet
        "Character/Face/00020116/angry/0/face",   // per-img atlas（映射問題案例）
        "Map/Obj/login/0",                        // spritesheet（若存在）
        "Character/Pants/01062105/alert/0/pants", // per-img?
    ];
    let cases: Vec<&str> = requested_path
        .as_deref()
        .map_or_else(|| default_cases.to_vec(), |path| vec![path]);
    std::fs::create_dir_all("out/imgdemo")?;
    for c in cases {
        let t = std::time::Instant::now();
        match svc.get_image(c) {
            Ok(Some(png)) => {
                let fname = format!("out/imgdemo/{}.png", c.replace('/', "_"));
                std::fs::write(&fname, &png)?;
                println!("OK   {c} → {} bytes（{:.2?}）", png.len(), t.elapsed());
            }
            Ok(None) => println!("NONE {c}（{:.2?}）", t.elapsed()),
            Err(e) => match svc.get_canvas_atlas_fallback(c) {
                Ok(Some(png)) => {
                    let fname = format!("out/imgdemo/{}_atlas.png", c.replace('/', "_"));
                    std::fs::write(&fname, &png)?;
                    println!(
                        "ATLAS {c} → {} bytes（精確 frame 失敗：{e}；{:.2?}）",
                        png.len(),
                        t.elapsed()
                    );
                }
                Ok(None) => println!("ERR  {c}: {e}"),
                Err(fallback_error) => {
                    println!("ERR  {c}: {e}；atlas fallback: {fallback_error}")
                }
            },
        }
    }

    // 二次開啟（快取命中計時）
    let (_svc2, info2) = DataService::open(&root, &cache)?;
    println!(
        "\n二次開啟: from_cache={} {}ms",
        info2.from_cache, info2.elapsed_ms
    );
    Ok(())
}

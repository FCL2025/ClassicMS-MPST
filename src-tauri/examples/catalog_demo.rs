//! 後端 command 層邏輯驗證（不依賴 GUI）：
//! 開啟（含快取）→ list_children → get_node → search → 二次開啟計時。
//!
//! 用法：
//! ```text
//! cargo run --release --example catalog_demo -- [bundle 目錄] [--rebuild]
//! ```

use std::path::PathBuf;

use msclassic::catalog::{ChildEntry, DataService};
use msclassic::wzjs::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w")
        });
    let cache = std::env::temp_dir().join("msclassic_catalog_demo.json");
    if args.iter().any(|a| a == "--rebuild") {
        std::fs::remove_file(&cache).ok();
    }

    // ── 第一次開啟（無快取時重建）──
    let (svc, info) = DataService::open(&root, &cache)?;
    println!(
        "開啟: from_cache={} 耗時={}ms entries={} bundles={}",
        info.from_cache, info.elapsed_ms, info.entry_count, info.bundle_count
    );

    // ── 頂層目錄 ──
    let top = svc.list_children("")?;
    println!("\n頂層目錄（{} 個）:", top.len());
    for c in top.iter().take(15) {
        match c {
            ChildEntry::Category {
                name, img_count, ..
            } => println!("  [D] {name}（{img_count} imgs）"),
            ChildEntry::Img { name, .. } => println!("  [I] {name}"),
            ChildEntry::Node { .. } => unreachable!(),
        }
    }

    // ── Character 下一層（應為 7386+ imgs）──
    let t0 = std::time::Instant::now();
    let ch = svc.list_children("Character")?;
    println!("Character/ 下 {} 個項目（{:.2?}）", ch.len(), t0.elapsed());

    // ── 懶解碼一個 img 並導航 ──
    let t0 = std::time::Instant::now();
    let blank = svc.get_node("Character/Afterimage/blank")?;
    println!(
        "\nCharacter/Afterimage/blank: type={} value={:?} 子節點 {} 個（首次解碼 {:.2?}）",
        blank.type_tag,
        blank.value,
        blank.children.len(),
        t0.elapsed()
    );
    let names: Vec<&str> = blank.children.iter().map(|c| c.0.as_str()).collect();
    println!("  子節點: {names:?}");

    // 深層導航 + 型別展示
    let delay = svc.get_node("Character/Face/00020116/blink/0/delay")?;
    println!("\nFace/00020116/blink/0/delay = {:?}", delay.value);
    let origin = svc.get_node("Character/Face/00020116/blink/0/face/origin")?;
    println!("Face/00020116/blink/0/face/origin = {:?}", origin.value);
    let link = svc.get_node("Character/Pants/01062105/heal/0/pants")?;
    println!("Pants/01062105/heal/0/pants = {:?}", link.value);

    // ── 快取命中後的懶解碼速度 ──
    let t0 = std::time::Instant::now();
    let _ = svc.get_node("Character/Afterimage/blank")?;
    println!("\n二次取同 img（樹快取命中）: {:.2?}", t0.elapsed());

    // ── TextAsset ──
    match svc.get_node("Sound/ItemTable") {
        Ok(t) => println!(
            "\nItemTable: class={} text 前 80 字 = {:?}",
            t.type_tag,
            t.text_asset.as_deref().map(|s| &s[..s.len().min(80)])
        ),
        Err(e) => println!("\nItemTable 取得失敗: {e}"),
    }

    // ── 搜尋 ──
    let t0 = std::time::Instant::now();
    let hits = svc.search("afterimage", 10);
    println!(
        "\nsearch(\"afterimage\") → {} 筆（{:.2?}）: {:?}",
        hits.len(),
        t0.elapsed(),
        hits
    );
    let hits = svc.search("00020116", 10);
    println!("search(\"00020116\") → {} 筆: {:?}", hits.len(), hits);

    // ── 第二次開啟（快取命中，應秒開）──
    let (svc2, info2) = DataService::open(&root, &cache)?;
    println!(
        "\n二次開啟: from_cache={} 耗時={}ms entries={}",
        info2.from_cache, info2.elapsed_ms, info2.entry_count
    );
    let svc2 = svc2;
    let top2 = svc2.list_children("")?;
    println!("二次開啟頂層目錄數: {}", top2.len());

    let _ = Value::SubTree; // silence unused import in some cfgs
    Ok(())
}

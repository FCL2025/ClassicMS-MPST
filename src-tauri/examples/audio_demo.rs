//! 聲音功能驗證：建索引（含 mp3）→ get_audio → 輸出 ogg。
//! 用法: cargo run --release --example audio_demo -- [bundle 目錄] [輸出 ogg 目錄]

use std::path::PathBuf;

use msclassic::catalog::DataService;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w")
        });
    let out_dir = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let cache = std::env::temp_dir().join("msclassic_catalog_audio_demo.json");
    std::fs::remove_file(&cache).ok(); // 強制重建,驗證新版索引

    let (svc, info) = DataService::open(&root, &cache)?;
    println!(
        "開啟: entries={} bundles={} 耗時={}ms",
        info.entry_count, info.bundle_count, info.elapsed_ms
    );

    // Sound 類別下應同時有 *Table（TextAsset）與音檔子目錄
    let sound_children = svc.list_children("Sound")?;
    println!("Sound/ 下 {} 項:", sound_children.len());
    for c in sound_children.iter().take(10) {
        match c {
            msclassic::catalog::ChildEntry::Category {
                name, img_count, ..
            } => println!("  [D] {name}（{img_count}）"),
            msclassic::catalog::ChildEntry::Img { name, class_id, .. } => {
                println!("  [I] {name} class={class_id}")
            }
            msclassic::catalog::ChildEntry::Node { .. } => unreachable!(),
        }
    }

    for path in ["Sound/Bgm00/DragonDream", "Sound/Item/02000000/Use"] {
        let node = svc.get_node(path)?;
        println!(
            "\n{path}: name={:?} type={} children={}",
            node.name,
            node.type_tag,
            node.children.len()
        );
        let t0 = std::time::Instant::now();
        let ogg = svc.get_audio(path)?;
        let out = out_dir.join(format!("{}.ogg", node.name));
        std::fs::write(&out, &ogg)?;
        println!(
            "  → {} bytes ogg（{:.2?}）→ {}",
            ogg.len(),
            t0.elapsed(),
            out.display()
        );
        println!("  magic: {:?}", &ogg[..4]);
    }
    Ok(())
}

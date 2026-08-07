//! ItemEff 特效合成驗證：穿特效披風跑 avatar_frames，把場景 JSON 與
//! 所有圖層 PNG 匯出到 out/itemeff/render/，由 scripts/itemeff_composite.mjs
//! （Node + sharp）合成最終預覽圖。
//!
//! 用法：cargo run --release --example itemeff_render -- [cape_id]
//! （預設 01103617 仲夏夜蝴蝶）

use std::path::PathBuf;

use msclassic::avatar::{avatar_frames, PartInput};
use msclassic::catalog::DataService;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w",
    );
    let cache = std::env::temp_dir().join("msclassic_catalog_demo.json");
    let (svc, _) = DataService::open(&root, &cache)?;
    let cape = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "01103617".to_string());

    let parts = vec![
        PartInput {
            slot: "body".into(),
            id: "00002000".into(),
            visible: true,
        },
        PartInput {
            slot: "head".into(),
            id: "00012000".into(),
            visible: true,
        },
        PartInput {
            slot: "face".into(),
            id: "00020000".into(),
            visible: true,
        },
        PartInput {
            slot: "hair".into(),
            id: "00030000".into(),
            visible: true,
        },
        PartInput {
            slot: "cape".into(),
            id: cape.clone(),
            visible: true,
        },
    ];
    let frames = avatar_frames(&svc, parts, "stand1", "default", true, true)?;
    println!(
        "stand1: body {} 幀 / face {} 幀 / effect {} 幀",
        frames.frames.len(),
        frames.face_frames.len(),
        frames.effect_frames.len()
    );
    for (j, f) in frames.effect_frames.iter().take(3).enumerate() {
        for l in &f.layers {
            println!(
                "  effect 幀{j}: delay={} z={} x={:.1} y={:.1} {}",
                f.delay, l.z, l.x, l.y, l.path
            );
        }
    }

    // 匯出場景 JSON + 所有圖層 PNG
    let out = PathBuf::from("out/itemeff/render");
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("scene.json"), serde_json::to_string(&frames)?)?;
    let mut paths: Vec<String> = Vec::new();
    for f in frames
        .frames
        .iter()
        .chain(&frames.face_frames)
        .chain(&frames.effect_frames)
    {
        for l in &f.layers {
            if !paths.contains(&l.path) {
                paths.push(l.path.clone());
            }
        }
    }
    for p in &paths {
        match svc.get_image(p) {
            Ok(Some(png)) => {
                std::fs::write(out.join(format!("{}.png", p.replace('/', "__"))), png)?;
            }
            other => println!("  !! 取圖失敗 {p}: {:?}", other.map(|o| o.map(|b| b.len()))),
        }
    }
    println!("匯出 {} 張圖層到 {}", paths.len(), out.display());
    Ok(())
}

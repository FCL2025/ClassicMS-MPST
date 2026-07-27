//! 紙娃娃核心 smoke test：用實機資料跑 avatar_meta / avatar_equipment /
//! avatar_frames，印出結果供人工檢查合理性。
//!
//! 用法：
//! ```text
//! cargo run --release --example avatar_smoke -- [bundle 目錄]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use msclassic::avatar::{avatar_emotions, avatar_equipment, avatar_frames, avatar_meta, PartInput};
use msclassic::catalog::DataService;
use msclassic::wzjs::Value;

/// 直接從 body img 讀某幀 body canvas 的 origin（驗證基準點固定用）。
fn body_origin(svc: &DataService, action: &str, i: usize) -> Option<(f32, f32)> {
    let tree = svc.tree_for("Character/00002000").ok()?;
    let node = tree.find(&format!("{action}/{i}/body/origin"))?;
    match node.value {
        Value::Vector(x, y) => Some((x, y)),
        _ => None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w")
    });
    let cache = std::env::temp_dir().join("msclassic_catalog_demo.json");
    let (svc, info) = DataService::open(&root, &cache)?;
    println!(
        "開啟: from_cache={} entries={}",
        info.from_cache, info.entry_count
    );

    // ── meta ──
    let meta = avatar_meta(&svc)?;
    println!("\n== meta ==");
    println!(
        "actions: {} 個，前 10: {:?}",
        meta.actions.len(),
        &meta.actions[..meta.actions.len().min(10)]
    );
    println!("skins: {} 個，前 5:", meta.skins.len());
    for s in meta.skins.iter().take(5) {
        println!("  body={} head={} name={}", s.body_id, s.head_id, s.name);
    }

    // ── equipment（冷/熱計時 + cash 統計）──
    let t0 = std::time::Instant::now();
    let equipment = avatar_equipment(&svc)?;
    let cold = t0.elapsed();
    let t1 = std::time::Instant::now();
    let equipment_hot = avatar_equipment(&svc)?;
    let hot = t1.elapsed();
    let mut by_slot: BTreeMap<&str, usize> = BTreeMap::new();
    for item in &equipment {
        *by_slot.entry(item.slot.as_str()).or_default() += 1;
    }
    println!(
        "\n== equipment（共 {} 件）冷載入 {cold:.2?} / 熱載入 {hot:.2?} ==",
        equipment.len()
    );
    for (slot, count) in &by_slot {
        println!("  {slot}: {count}");
    }
    if let Some(first) = equipment.first() {
        println!(
            "  範例: id={} name={} slot={} icon={}",
            first.id, first.name, first.slot, first.icon_path
        );
    }
    let cash_items: Vec<_> = equipment.iter().filter(|e| e.cash).collect();
    let cash_hot = equipment_hot.iter().filter(|e| e.cash).count();
    println!(
        "cash=true: {} 件（熱載入核對: {cash_hot}），前 5 件:",
        cash_items.len()
    );
    for item in cash_items.iter().take(5) {
        println!(
            "  cash 範例: id={} name={} slot={}",
            item.id, item.name, item.slot
        );
    }

    // ── emotions ──
    let emotions = avatar_emotions(&svc, "00020000")?;
    println!(
        "\n== face 00020000 表情（{} 個）: {:?}",
        emotions.len(),
        emotions
    );

    // ── stand1 第 0 幀 ──
    let parts = || {
        vec![
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
                slot: "coat".into(),
                id: "01040036".into(),
                visible: true,
            },
            PartInput {
                slot: "pants".into(),
                id: "01060026".into(),
                visible: true,
            },
        ]
    };
    let stand = avatar_frames(&svc, parts(), "stand1", "default", true)?;
    println!(
        "\n== stand1: {} 幀（face_frames {} 幀）==",
        stand.frames.len(),
        stand.face_frames.len()
    );
    let frame0 = &stand.frames[0];
    println!(
        "幀 0: delay={}，{} layers（z 遞減），faceAnchor={:?}，showFace={}:",
        frame0.delay,
        frame0.layers.len(),
        frame0.face_anchor,
        frame0.show_face
    );
    for l in &frame0.layers {
        println!("  z={:>4} x={:>7.1} y={:>7.1} {}", l.z, l.x, l.y, l.path);
    }
    if let Some(face0) = stand.face_frames.first() {
        println!(
            "face 幀 0: delay={}，{} layers（相對 brow 的相對座標）:",
            face0.delay,
            face0.layers.len()
        );
        for l in &face0.layers {
            println!("  z={:>4} x={:>7.1} y={:>7.1} {}", l.z, l.x, l.y, l.path);
        }
    }

    // ── walk1 全部幀：驗證基準點固定（body origin 絕對位置應各幀相同）──
    let walk = avatar_frames(&svc, parts(), "walk1", "default", true)?;
    println!(
        "\n== walk1: {} 幀（face_frames {} 幀）==",
        walk.frames.len(),
        walk.face_frames.len()
    );
    for (i, f) in walk.frames.iter().enumerate() {
        // body layer 的位置即該幀 shift
        let body_layer = f.layers.iter().find(|l| l.path.ends_with("/body"));
        let (shift, origin_abs) = match (body_layer, body_origin(&svc, "walk1", i)) {
            (Some(l), Some(o)) => (
                format!("({:.1},{:.1})", l.x, l.y),
                format!("({:.1},{:.1})", l.x + o.0, l.y + o.1),
            ),
            _ => ("?".into(), "?".into()),
        };
        println!(
            "  幀{i}: {} layers, delay={}, shift={shift}, body origin 絕對位置={origin_abs}, faceAnchor={:?}, showFace={}",
            f.layers.len(),
            f.delay,
            f.face_anchor,
            f.show_face
        );
    }
    for (j, f) in walk.face_frames.iter().enumerate() {
        println!(
            "  face 幀{j}: delay={}，{} layers（相對座標）:",
            f.delay,
            f.layers.len()
        );
        for l in &f.layers {
            println!("    z={:>4} x={:>7.1} y={:>7.1} {}", l.z, l.x, l.y, l.path);
        }
    }

    Ok(())
}

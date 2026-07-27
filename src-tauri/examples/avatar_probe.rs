//! 紙娃娃前置探查：Character WZJS 的實際節點結構。
//!
//! 用法：
//! ```text
//! cargo run --release --example avatar_probe -- [bundle 目錄]
//! ```

use std::collections::BTreeSet;
use std::path::PathBuf;

use msclassic::catalog::{ChildEntry, DataService};
use msclassic::wzjs::{Value, WzNode};

fn val_short(v: &Value) -> String {
    match v {
        Value::SubTree => "SubTree".into(),
        Value::Canvas => "Canvas".into(),
        Value::U8(n) => format!("U8({n})"),
        Value::U16(n) => format!("U16({n})"),
        Value::Int(n) => format!("Int({n})"),
        Value::Float2(a, b) => format!("Float2({a},{b})"),
        Value::Int2(a, b) => format!("Int2({a},{b})"),
        Value::Double(d) => format!("Double({d})"),
        Value::Vector(x, y) => format!("Vector({x},{y})"),
        Value::Str(s) => format!("Str({s:?})"),
        Value::Uol(s) => format!("Uol({s:?})"),
    }
}

fn dump_frame(svc: &DataService, img: &str, frame_path: &str, zs: &mut BTreeSet<String>) {
    let Ok(tree) = svc.tree_for(img) else {
        println!("  !! tree_for({img}) 失敗");
        return;
    };
    let Some(frame) = tree.find(frame_path) else {
        println!("  !! {img} 找不到 {frame_path}");
        return;
    };
    println!("  [{img}/{frame_path}] value={}", val_short(&frame.value));
    for c in &frame.children {
        if matches!(c.value, Value::Canvas | Value::Uol(_)) {
            let z = c
                .find("z")
                .map(|n| val_short(&n.value))
                .unwrap_or("-".into());
            if let Some(Value::Str(s)) = c.find("z").map(|n| &n.value) {
                zs.insert(s.clone());
            }
            let origin = c
                .find("origin")
                .map(|n| val_short(&n.value))
                .unwrap_or("-".into());
            let map_keys: Vec<String> = c
                .find("map")
                .map(|m| {
                    m.children
                        .iter()
                        .map(|k| format!("{}={}", k.name, val_short(&k.value)))
                        .collect()
                })
                .unwrap_or_default();
            println!(
                "    canvas {:<22} z={:<28} origin={:<18} map=[{}]",
                c.name,
                z,
                origin,
                map_keys.join(", ")
            );
        } else {
            println!("    attr {:<22} {}", c.name, val_short(&c.value));
        }
    }
}

fn collect_z(tree: &WzNode, zs: &mut BTreeSet<String>) {
    if tree.name == "z" {
        if let Value::Str(s) = &tree.value {
            zs.insert(s.clone());
        }
    }
    for c in &tree.children {
        collect_z(c, zs);
    }
}

/// 遞迴 dump（限深度），顯示每個節點的值。
fn dump_tree(n: &WzNode, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }
    let indent = "  ".repeat(depth);
    let kids = if n.children.is_empty() {
        String::new()
    } else {
        format!(" ({} 子)", n.children.len())
    };
    println!(
        "{indent}{}{} [{}]{kids}",
        n.name,
        val_short(&n.value),
        n.type_tag
    );
    for c in &n.children {
        dump_tree(c, depth + 1, max_depth);
    }
}

fn probe_tree(svc: &DataService, img: &str, path: &str, max_depth: usize) {
    let Ok(tree) = svc.tree_for(img) else {
        println!("  !! tree_for({img}) 失敗");
        return;
    };
    match tree.find(path) {
        Some(n) => dump_tree(n, 0, max_depth),
        None => println!("  !! {img} 找不到 {path}"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w")
        });
    let cache = std::env::temp_dir().join("msclassic_catalog_demo.json");
    let (svc, info) = DataService::open(&root, &cache)?;
    println!(
        "開啟: from_cache={} entries={}",
        info.from_cache, info.entry_count
    );

    // ── 1. Character 子目錄 ──
    let ch = svc.list_children("Character")?;
    println!("\n== Character 子目錄 ==");
    for c in &ch {
        match c {
            ChildEntry::Category {
                name, img_count, ..
            } => println!("  [D] {name}（{img_count}）"),
            ChildEntry::Img { name, .. } => println!("  [I] {name}"),
            ChildEntry::Node { .. } => {}
        }
    }

    // ── 2. body img 動作清單 ──
    let body = svc.tree_for("Character/00002000")?;
    let actions: Vec<&str> = body
        .children
        .iter()
        .filter(|c| c.name != "info")
        .map(|c| c.name.as_str())
        .collect();
    println!("\n== 00002000 動作（{} 個）==\n{actions:?}", actions.len());

    // ── 3. 幀結構 dump ──
    let mut zs = BTreeSet::new();
    println!("\n== body stand1/0 ==");
    dump_frame(&svc, "Character/00002000", "stand1/0", &mut zs);
    println!("\n== body walk1/0 ==");
    dump_frame(&svc, "Character/00002000", "walk1/0", &mut zs);

    // ── 4. head / hair / face 結構 ──
    for img in [
        "Character/00012000",
        "Character/Hair/00030000",
        "Character/Face/00020000",
    ] {
        match svc.tree_for(img) {
            Ok(t) => {
                let names: Vec<&str> = t.children.iter().map(|c| c.name.as_str()).collect();
                println!("\n== {img} 子節點 ==\n{names:?}");
            }
            Err(e) => println!("\n== {img} 失敗: {e}"),
        }
    }
    println!("\n== head front/0 ==");
    dump_frame(&svc, "Character/00012000", "front/0", &mut zs);
    println!("\n== hair stand1/0 ==");
    dump_frame(&svc, "Character/Hair/00030000", "stand1/0", &mut zs);
    println!("\n== face default/0 ==");
    dump_frame(&svc, "Character/Face/00020000", "default/0", &mut zs);

    // ── 4b. 深層結構探查：head front / face default / hair / cap ──
    println!("\n== head front 全結構 ==");
    probe_tree(&svc, "Character/00012000", "front", 3);
    println!("\n== head stand1/0 ==");
    probe_tree(&svc, "Character/00012000", "stand1/0", 3);
    println!("\n== face default 全結構 ==");
    probe_tree(&svc, "Character/Face/00020000", "default", 3);
    println!("\n== face blink/0 ==");
    probe_tree(&svc, "Character/Face/00020000", "blink/0", 3);
    println!("\n== hair stand1/0 全結構 ==");
    probe_tree(&svc, "Character/Hair/00030000", "stand1/0", 3);
    println!("\n== hair default/0 全結構 ==");
    probe_tree(&svc, "Character/Hair/00030000", "default/0", 3);
    println!("\n== cap default 全結構 ==");
    probe_tree(&svc, "Character/Cap/01000000", "default", 3);
    println!("\n== cap stand1/0 全結構 ==");
    probe_tree(&svc, "Character/Cap/01000000", "stand1/0", 3);
    println!("\n== body stand1/0 全結構（對照） ==");
    probe_tree(&svc, "Character/00002000", "stand1/0", 3);

    // ── 5. 裝備範例（從 catalog 動態挑每目錄第一個）──
    for dir in [
        "Cap", "Coat", "Longcoat", "Pants", "Shoes", "Glove", "Shield", "Cape", "Weapon",
    ] {
        let path = format!("Character/{dir}");
        match svc.list_children(&path) {
            Ok(items) => {
                let first = items.iter().find_map(|c| match c {
                    ChildEntry::Img { name, .. } => Some(name.clone()),
                    _ => None,
                });
                println!("\n== {dir} 共 {} 個，範例 {first:?} ==", items.len());
                if let Some(id) = first {
                    let img = format!("Character/{dir}/{id}");
                    if let Ok(t) = svc.tree_for(&img) {
                        let names: Vec<&str> = t.children.iter().map(|c| c.name.as_str()).collect();
                        println!("   子節點: {names:?}");
                        dump_frame(&svc, &img, "stand1/0", &mut zs);
                    }
                }
            }
            Err(e) => println!("\n== {dir}: {e}"),
        }
    }

    // ── 6. accessory 目錄探查 ──
    println!("\n== 其他 Character 子目錄（配件類）==");
    for c in &ch {
        if let ChildEntry::Category {
            name, img_count, ..
        } = c
        {
            if ![
                "Cap",
                "Coat",
                "Longcoat",
                "Pants",
                "Shoes",
                "Glove",
                "Shield",
                "Cape",
                "Weapon",
                "Hair",
                "Face",
                "Afterimage",
                "TamingMob",
                "Pet",
            ]
            .contains(&name.as_str())
            {
                println!("  [D] {name}（{img_count}）");
            }
        }
    }
    // Accessory 的 id 前綴分布
    if let Ok(acc) = svc.list_children("Character/Accessory") {
        let mut prefixes: std::collections::BTreeMap<String, u32> = Default::default();
        let mut samples: Vec<String> = Vec::new();
        for c in &acc {
            if let ChildEntry::Img { name, .. } = c {
                if name.len() >= 4 {
                    *prefixes.entry(name[..4].to_string()).or_default() += 1;
                }
                if samples.len() < 5 {
                    samples.push(name.clone());
                }
            }
        }
        println!("Accessory 前 4 碼分布: {prefixes:?}");
        println!("Accessory 範例: {samples:?}");
        if let Some(first) = samples.first() {
            println!("== Accessory/{first} 結構 ==");
            let img = format!("Character/Accessory/{first}");
            if svc.tree_for(&img).is_ok() {
                probe_tree(&svc, &img, "stand1/0", 3);
            }
        }
    }
    // Ring 目錄是否為特效戒（不可見）
    if let Ok(rings) = svc.list_children("Character/Ring") {
        let names: Vec<String> = rings
            .iter()
            .filter_map(|c| match c {
                ChildEntry::Img { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        println!("Ring imgs: {names:?}");
    }

    // ── 7. 全部 z 字串集合（body + 範例裝備）──
    collect_z(&body, &mut zs);
    println!("\n== 收集到的 z 圖層名（{} 個）==\n{zs:?}", zs.len());

    // ── 8. String/TW/Item 名稱涵蓋率 ──
    match svc.get_node("String/TW/Item") {
        Ok(t) => {
            if let Some(text) = t.text_asset.as_deref() {
                println!("\n== String/TW/Item 長度 {} ==", text.len());
                println!("前 300 字: {}", &text[..text.len().min(300)]);
                for probe in ["1040036", "1000003", "20000", "30000", "1060026"] {
                    println!("  含 {probe}: {}", text.contains(probe));
                }
            } else {
                println!("\nString/TW/Item 無 text_asset，value={:?}", t.value);
            }
        }
        Err(e) => println!("\nString/TW/Item: {e}"),
    }
    // 找其他可能的名稱表
    if let Ok(top) = svc.list_children("String/TW") {
        let names: Vec<String> = top
            .iter()
            .map(|c| match c {
                ChildEntry::Category { name, .. } => format!("D:{name}"),
                ChildEntry::Img { name, .. } => format!("I:{name}"),
                ChildEntry::Node { name, .. } => format!("N:{name}"),
            })
            .collect();
        println!("String/TW 下: {names:?}");
    }

    Ok(())
}

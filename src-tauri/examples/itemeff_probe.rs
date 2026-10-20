//! 探查 Effect/ItemEff 的實際節點結構（披風特效）。
//!
//! 用法：
//! ```text
//! cargo run --release --example itemeff_probe -- [bundle 目錄]
//! ```

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
        Value::Long(n) => format!("Long({n})"),
        Value::Float2(a, b) => format!("Float2({a},{b})"),
        Value::Int2(a, b) => format!("Int2({a},{b})"),
        Value::Double(d) => format!("Double({d})"),
        Value::Vector(x, y) => format!("Vector({x},{y})"),
        Value::Str(s) => format!("Str({s:?})"),
        Value::Uol(s) => format!("Uol({s:?})"),
    }
}

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

    // ── 1. Effect 目錄 ──
    println!("\n== Effect 子目錄 ==");
    for c in svc.list_children("Effect")? {
        match c {
            ChildEntry::Category {
                name, img_count, ..
            } => println!("  [D] {name}（{img_count}）"),
            ChildEntry::Img { name, .. } => println!("  [I] {name}"),
            ChildEntry::Node { name, .. } => println!("  [N] {name}"),
        }
    }

    // ── 2. ItemEff 是單一 img：子節點即道具 id ──
    let tree = svc.tree_for("Effect/ItemEff")?;
    let ids: Vec<String> = tree.children.iter().map(|c| c.name.clone()).collect();
    println!("\n== Effect/ItemEff 共 {} 條 ==", ids.len());
    let capes: Vec<&String> = ids.iter().filter(|id| id.starts_with("110")).collect();
    println!("披風（0110 開頭）{} 條: {capes:?}", capes.len());
    let others: Vec<&String> = ids.iter().filter(|id| !id.starts_with("110")).collect();
    println!("其他 {} 條: {others:?}", others.len());

    // ── 3. 逐條 dump 披風特效結構 ──
    for id in &capes {
        println!("\n===== Effect/ItemEff/{id} =====");
        match tree.find(id) {
            Some(n) => dump_tree(n, 0, 4),
            None => println!("  !! 找不到"),
        }
    }

    // ── 4. 抽一條 dump canvas 細節（z/origin/map/delay）──
    if let Some(id) = capes.first() {
        let Some(item) = tree.find(id) else {
            return Ok(());
        };
        for action in &item.children {
            if action.name == "info" {
                continue;
            }
            println!("\n== Effect/ItemEff/{id}/{} 幀結構 ==", action.name);
            for frame in &action.children {
                println!("  幀 {} [{}]", frame.name, val_short(&frame.value));
                for c in &frame.children {
                    let z = c
                        .find("z")
                        .map(|n| val_short(&n.value))
                        .unwrap_or("-".into());
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
                        "    {:<14} [{}] z={z} origin={origin} map=[{}]",
                        c.name,
                        val_short(&c.value),
                        map_keys.join(", ")
                    );
                }
            }
            break; // 只看第一個動作
        }
    }

    // ── 5. 對照組：披風本體 + body + 特效的圖片尺寸與錨點 ──
    let png_dims = |svc: &DataService, path: &str| -> String {
        match svc.get_image(path) {
            Ok(Some(png)) if png.len() > 24 => {
                let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
                let h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
                format!("{w}x{h}")
            }
            other => format!("取圖失敗: {:?}", other.map(|o| o.map(|p| p.len()))),
        }
    };

    for cape_id in ["01103617", "01103157", "01103155", "01103017"] {
        let img = format!("Character/Cape/{cape_id}");
        println!("\n== {img} stand1/0 ==");
        let Ok(t) = svc.tree_for(&img) else {
            println!("  !! 無此披風");
            continue;
        };
        let Some(frame) = t.find("stand1/0") else {
            println!("  !! 無 stand1/0");
            continue;
        };
        for c in &frame.children {
            let z = c
                .find("z")
                .map(|n| val_short(&n.value))
                .unwrap_or("-".into());
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
            let dims = png_dims(&svc, &format!("{img}/stand1/0/{}", c.name));
            println!(
                "    {:<14} [{}] z={z} origin={origin} dims={dims} map=[{}]",
                c.name,
                val_short(&c.value),
                map_keys.join(", ")
            );
        }
    }

    // body stand1/0：origin / map / dims
    println!("\n== Character/00002000 stand1/0 ==");
    let body = svc.tree_for("Character/00002000")?;
    if let Some(frame) = body.find("stand1/0") {
        for c in &frame.children {
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
            let dims = png_dims(&svc, &format!("Character/00002000/stand1/0/{}", c.name));
            println!(
                "    {:<10} origin={origin} dims={dims} map=[{}]",
                c.name,
                map_keys.join(", ")
            );
        }
    }

    // 特效幀圖尺寸
    for eff_id in ["1103617", "1103157"] {
        for action in ["default", "walk1"] {
            let p = format!("Effect/ItemEff/{eff_id}/effect/{action}/0");
            println!("{p} dims={}", png_dims(&svc, &p));
        }
    }

    // 披風名稱（直接從 String/TW/Item 文字表查）
    if let Ok(t) = svc.get_node("String/TW/Item") {
        if let Some(text) = t.text_asset.as_deref() {
            for id in ["1103617", "1103157", "1103155", "1103017"] {
                let key = format!("\"{id}\"");
                let found = text.find(&key).map(|i| {
                    text.get(i..i + 120)
                        .map(|seg| seg.replace('\n', " "))
                        .unwrap_or_default()
                });
                println!("披風 0{id}: {}", found.unwrap_or("查無".into()));
            }
        }
    }

    Ok(())
}

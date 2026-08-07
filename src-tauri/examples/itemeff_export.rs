//! 匯出 ItemEff 特效幀 PNG + 披風本體 PNG 供目視檢查。
//!
//! 用法：cargo run --release --example itemeff_export

use std::path::PathBuf;

use msclassic::catalog::DataService;
use msclassic::wzjs::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w")
        });
    let cache = std::env::temp_dir().join("msclassic_catalog_demo.json");
    let (svc, _) = DataService::open(&root, &cache)?;

    // 各披風特效 default/0 的 origin 與 effect/action 層屬性
    let tree = svc.tree_for("Effect/ItemEff")?;
    let png_dims = |svc: &DataService, path: &str| -> String {
        match svc.get_image(path) {
            Ok(Some(png)) if png.len() > 24 => {
                let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
                let h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
                format!("{w}x{h}")
            }
            _ => "?".into(),
        }
    };
    for item in &tree.children {
        let id = &item.name;
        let Some(effect) = item.find("effect") else {
            continue;
        };
        let Some(action) = effect
            .children
            .iter()
            .find(|c| matches!(c.value, Value::SubTree))
        else {
            continue;
        };
        let z = action.find("z").map(|n| format!("{:?}", n.value));
        let pos = action.find("pos").map(|n| format!("{:?}", n.value));
        let origin = action
            .find("0")
            .and_then(|f| f.find("origin"))
            .map(|n| format!("{:?}", n.value));
        let dims = png_dims(
            &svc,
            &format!("Effect/ItemEff/{id}/effect/{}/0", action.name),
        );
        let show = |o: Option<String>| o.unwrap_or_else(|| "-".into());
        println!(
            "{id:<10} action={:<10} z={:<14} pos={:<14} origin={:<24} dims={dims}",
            action.name,
            show(z),
            show(pos),
            show(origin)
        );
    }
    println!();

    // head / body 錨點座標（推算 pos=1 的錨點語義）
    let head = svc.tree_for("Character/00012000")?;
    if let Some(canvas) = head.find("front/head") {
        let origin = canvas.find("origin").map(|n| format!("{:?}", n.value));
        let map: Vec<String> = canvas
            .find("map")
            .map(|m| {
                m.children
                    .iter()
                    .map(|c| format!("{}={:?}", c.name, c.value))
                    .collect()
            })
            .unwrap_or_default();
        println!(
            "head front/head origin={origin:?} map=[{}] dims={}",
            map.join(", "),
            png_dims(&svc, "Character/00012000/front/head")
        );
    }

    let out = PathBuf::from("out/itemeff");
    std::fs::create_dir_all(&out)?;

    let paths = [
        "Effect/ItemEff/1103617/effect/default/0",
        "Effect/ItemEff/1103617/effect/default/5",
        "Effect/ItemEff/1103617/effect/walk1/0",
        "Effect/ItemEff/1103157/effect/default/0",
        "Effect/ItemEff/1103155/effect/default/0",
        "Effect/ItemEff/1103017/effect/default/0",
        "Character/Cape/01103617/default/default",
        "Character/Cape/01103157/default/default",
        "Character/Cape/01103017/default/default",
        "Character/00002000/stand1/0/body",
    ];
    for p in paths {
        match svc.get_image(p) {
            Ok(Some(png)) => {
                let name = p.replace('/', "_") + ".png";
                std::fs::write(out.join(&name), &png)?;
                println!("OK {p} → {} ({} bytes)", name, png.len());
            }
            other => println!("!! {p}: {:?}", other.map(|o| o.map(|b| b.len()))),
        }
    }
    Ok(())
}

//! WZJS 解碼交叉比對（oracle = 舊版 v83 Character.wz）。
//!
//! 用法：
//! ```text
//! cargo run --release --example wzjs_xcheck -- <v83 Character.wz 路徑>
//! ```
//!
//! 比對方式：
//! 1. `Character/Afterimage/blank`：WZJS fixture 的 9 個子節點名稱必須與舊版完全一致
//! 2. 任選 20 筆節點（Face/00020116、Pants/01062105 的子樹），逐項核對
//!    節點名稱、階層與純量值（delay int、origin vector、uol 連結…）

use std::process::ExitCode;

use maple_lib::wz::container::directory::WzDirectory;
use maple_lib::wz::container::file::WzFile;
use maple_lib::wz::property::WzImageProperty;
use maple_lib::wz::types::WzMapleVersion;
use msclassic::wzjs::{fixtures, parse, Value, WzNode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法: wzjs_xcheck <v83 Character.wz>");
        return ExitCode::from(2);
    }

    let mut f = WzFile::open(args[0].clone(), WzMapleVersion::Gms);
    match f.parse_wz_file() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("WZ 開啟失敗: {e}");
            return ExitCode::FAILURE;
        }
    }

    let pass = std::cell::Cell::new(0usize);
    let fail = std::cell::Cell::new(0usize);
    let mut report = |ok: bool, msg: String| {
        println!("{} {msg}", if ok { "OK  " } else { "FAIL" });
        if ok {
            pass.set(pass.get() + 1)
        } else {
            fail.set(fail.get() + 1)
        }
    };

    // ── 1. blank：9 個子節點名稱完全一致 ──
    {
        let wzjs = parse(fixtures::BLANK).unwrap();
        let root = f.wz_directory_mut().expect("no root dir");
        let names_v83 = find_img_child_names(root, &["Afterimage", "blank"]);
        match names_v83 {
            Some(mut v83) => {
                v83.sort();
                let mut wz: Vec<String> = wzjs.children.iter().map(|c| c.name.clone()).collect();
                wz.sort();
                report(
                    v83 == wz,
                    format!("Afterimage/blank 子節點：v83={v83:?} wzjs={wz:?}"),
                );
            }
            None => report(false, "Afterimage/blank 在 v83 Character.wz 找不到".into()),
        }
    }

    // ── 2. Face/00020116 與 Pants/01062105 的 20 筆節點核對 ──
    {
        let face = parse(fixtures::IMG_00020116).unwrap();
        let pants = parse(fixtures::IMG_01062105).unwrap();
        let root = f.wz_directory_mut().expect("no root dir");

        // (WZJS 樹, v83 路徑, 說明)
        let cases: Vec<(&WzNode, &[&str], &str)> =
            vec![(&face, &["Face", "00020116"], "Face img 根")];
        for (wzjs_root, v83_path, desc) in cases {
            compare_subtree(
                root,
                v83_path,
                wzjs_root,
                desc,
                20usize.saturating_sub(pass.get() + fail.get()),
                &mut report,
            );
        }
        // Pants
        compare_subtree(
            root,
            &["Pants", "01062105"],
            &pants,
            "Pants img 根",
            20usize.saturating_sub(pass.get() + fail.get()),
            &mut report,
        );
    }

    println!("\n結果: {} 通過 / {} 失敗", pass.get(), fail.get());
    if fail.get() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// 在 v83 WZ 目錄樹裡找 img，回傳它頂層 property 名稱列表。
fn find_img_child_names(root: &mut WzDirectory, path: &[&str]) -> Option<Vec<String>> {
    let img = find_img(root, path)?;
    Some(
        img.properties
            .iter()
            .map(|p| p.name().to_string())
            .collect(),
    )
}

fn find_img<'a>(
    root: &'a mut WzDirectory,
    path: &[&str],
) -> Option<&'a mut maple_lib::wz::container::image::WzImage> {
    let mut dir = root;
    for (i, seg) in path.iter().enumerate() {
        if i == path.len() - 1 {
            let img = dir.get_image_mut(&format!("{seg}.img"))?;
            img.parse_image().ok()?;
            return Some(img);
        }
        dir = dir.get_directory_mut(seg)?;
    }
    None
}

/// 逐項比對 WZJS 子樹與 v83 同路徑子樹（名稱集合 + 純量值），最多 max 筆。
fn compare_subtree(
    root: &mut WzDirectory,
    v83_path: &[&str],
    wzjs: &WzNode,
    desc: &str,
    max: usize,
    report: &mut dyn FnMut(bool, String),
) {
    let img = match find_img(root, v83_path) {
        Some(i) => i,
        None => {
            report(false, format!("{desc}: v83 找不到 {}", v83_path.join("/")));
            return;
        }
    };
    // 拍平 v83 property 樹成 path -> 簡單值描述
    let mut v83_map: Vec<(String, String)> = Vec::new();
    for p in &img.properties {
        flatten_prop(v83_path.last().unwrap(), p, &mut v83_map);
    }
    // 拍平 WZJS 樹
    let mut wz_map: Vec<(String, String)> = Vec::new();
    flatten_wzjs("", wzjs, &mut wz_map);

    let mut compared = 0;
    for (wpath, wval) in &wz_map {
        if compared >= max {
            break;
        }
        if wpath == &wzjs.name {
            continue; // 根節點本身：v83 flatten 不含 img 根，跳過
        }
        let hit = v83_map.iter().find(|(vp, _)| vp == wpath);
        match hit {
            Some((_, vval)) => {
                let ok = vval == wval || vval.starts_with("CANVAS") && wval == "CANVAS";
                report(
                    ok,
                    format!("{} /{wpath}: v83={vval} wzjs={wval}", v83_path.join("/")),
                );
            }
            None => report(
                false,
                format!(
                    "{} /{wpath}: v83 缺此節點（wzjs={wval}）",
                    v83_path.join("/")
                ),
            ),
        }
        compared += 1;
    }
}

/// v83 property 樹拍平成（路徑, 值描述）。
fn flatten_prop(prefix: &str, prop: &WzImageProperty, out: &mut Vec<(String, String)>) {
    let path = format!("{prefix}/{}", prop.name());
    let val = match prop {
        WzImageProperty::Null(_) => "NULL".into(),
        WzImageProperty::Short(p) => format!("INT {}", p.value),
        WzImageProperty::Int(p) => format!("INT {}", p.value),
        WzImageProperty::Long(p) => format!("INT {}", p.value),
        WzImageProperty::Float(p) => format!("FLOAT {}", p.value),
        WzImageProperty::Double(p) => format!("DOUBLE {}", p.value),
        WzImageProperty::String(p) => format!("STR {:?}", p.value),
        WzImageProperty::Uol(p) => format!("STR {:?}", p.value),
        WzImageProperty::Vector(p) => format!("VEC ({},{})", p.x, p.y),
        WzImageProperty::Canvas(_) => "CANVAS".into(),
        WzImageProperty::Sub(_) | WzImageProperty::Convex(_) => "SUB".into(),
        _ => "OTHER".into(),
    };
    out.push((path.clone(), val));
    if let Some(children) = prop.children() {
        for c in children {
            flatten_prop(&path, c, out);
        }
    }
}

/// WZJS 樹拍平成（路徑, 值描述），格式與 v83 對齊。
fn flatten_wzjs(prefix: &str, node: &WzNode, out: &mut Vec<(String, String)>) {
    let path = if prefix.is_empty() {
        node.name.clone()
    } else {
        format!("{prefix}/{}", node.name)
    };
    let val = match &node.value {
        Value::SubTree => "SUB".into(),
        Value::Canvas => "CANVAS".into(),
        Value::U8(v) => format!("INT {v}"),
        Value::U16(v) => format!("INT {v}"),
        Value::Int(v) => format!("INT {v}"),
        Value::Long(v) => format!("LONG {v}"),
        Value::Float2(a, b) => format!("FLOAT2 ({a},{b})"),
        Value::Int2(a, b) => format!("INT2 ({a},{b})"),
        Value::Double(v) => format!("DOUBLE {v}"),
        Value::Vector(x, y) => format!("VEC ({},{})", *x as i32, *y as i32),
        Value::Str(s) | Value::Uol(s) => format!("STR {s:?}"),
    };
    out.push((path.clone(), val));
    for c in &node.children {
        flatten_wzjs(&path, c, out);
    }
}

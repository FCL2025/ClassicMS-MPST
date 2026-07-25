//! spritesheet frame 映射 ground truth：用 v83 oracle 逐像素比對，
//! 找出 WZSS rect_j ↔ canvas path 的對應，並檢驗排序假設。
//!
//! 用法：
//! ```text
//! cargo run --release --example frame_map -- <v83 Character.wz> <wzss bin> <atlas page.png> <wzjs bin> [WZ directory] [image id]
//! ```

use maple_lib::wz::container::file::WzFile;
use maple_lib::wz::property::WzImageProperty;
use maple_lib::wz::types::WzMapleVersion;
use msclassic::image::parse_wzss;
use msclassic::wzjs::{self, Value, WzNode};

/// v83 property 樹裡收集所有 canvas（路徑, RGBA）。
fn collect_v83(prop: &WzImageProperty, path: &str, out: &mut Vec<(String, image::RgbaImage)>) {
    if let WzImageProperty::Canvas(c) = prop {
        if let Ok(img) = c.png.decompress() {
            out.push((path.to_string(), img));
        }
    }
    if let Some(children) = prop.children() {
        for ch in children {
            collect_v83(ch, &format!("{path}/{}", ch.name()), out);
        }
    }
}

fn collect_wzjs_canvas(n: &WzNode, out: &mut Vec<String>) {
    if matches!(n.value, Value::Canvas) {
        out.push(n.path.clone().unwrap_or_default());
    }
    for c in &n.children {
        collect_wzjs_canvas(c, out);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!(
            "用法: frame_map <v83 Character.wz> <wzss bin> <atlas png> <wzjs bin> [WZ directory] [image id]"
        );
        std::process::exit(2);
    }
    let wz_directory = args.get(4).map_or("Face", String::as_str);
    let image_id = args.get(5).map_or("00020116", String::as_str);

    // 1. v83 canvas 圖
    let mut f = WzFile::open(args[0].clone(), WzMapleVersion::Gms);
    f.parse_wz_file().expect("v83 開啟失敗");
    let root = f.wz_directory_mut().unwrap();
    let img = root
        .get_directory_mut(wz_directory)
        .and_then(|d| d.get_image_mut(&format!("{image_id}.img")))
        .unwrap_or_else(|| panic!("{wz_directory}/{image_id}.img 不存在"));
    img.parse_image().expect("img parse 失敗");
    let mut v83: Vec<(String, image::RgbaImage)> = Vec::new();
    for p in &img.properties {
        collect_v83(p, p.name(), &mut v83);
    }
    // v83 路徑是從 img 根以下（angry/0/face），strip 第一層（頂層 property 名）已含
    println!("v83 canvas 數：{}", v83.len());

    // 2. WZSS rects
    let wzss_data = std::fs::read(&args[1]).unwrap();
    let wzss = parse_wzss(&wzss_data).unwrap();
    println!("WZSS rects：{}", wzss.rects.len());

    // 3. atlas page（top-down RGBA）
    let atlas = image::open(&args[2])
        .expect("atlas png 開啟失敗")
        .to_rgba8();
    let (_, ah) = atlas.dimensions();

    // 4. WZJS canvas 路徑（DFS 順序）
    let tree = wzjs::parse(&std::fs::read(&args[3]).unwrap()).unwrap();
    let mut canvas_dfs = Vec::new();
    collect_wzjs_canvas(&tree, &mut canvas_dfs);
    println!("WZJS canvas（DFS）：{}", canvas_dfs.len());

    // 5. 逐 rect 比對
    let mut mapping: Vec<Option<String>> = Vec::new();
    for r in &wzss.rects {
        // bottom-up → top-down
        let ty = ah - r.y - r.h;
        let cut = image::imageops::crop_imm(&atlas, r.x, ty, r.w, r.h).to_image();
        let mut found = None;
        for (path, img) in &v83 {
            if img.dimensions() == (r.w, r.h) && img.as_raw() == cut.as_raw() {
                found = Some(path.clone());
                break;
            }
        }
        if found.is_none() {
            // 尺寸一致但像素不同 → 找最近似的（平均絕對差）
            let mut best: Option<(String, u64)> = None;
            for (path, img) in &v83 {
                if img.dimensions() != (r.w, r.h) {
                    continue;
                }
                let diff: u64 = img
                    .as_raw()
                    .iter()
                    .zip(cut.as_raw().iter())
                    .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as u64)
                    .sum();
                if best.as_ref().is_none_or(|(_, d)| diff < *d) {
                    best = Some((path.clone(), diff));
                }
            }
            if let Some((p, d)) = best {
                found = Some(format!("{p} (近似 diff={d})"));
            }
        }
        mapping.push(found);
    }
    for (j, m) in mapping.iter().enumerate() {
        let r = &wzss.rects[j];
        println!(
            "rect[{j:2}] ({}x{}) → {}",
            r.w,
            r.h,
            m.as_deref().unwrap_or("NO MATCH")
        );
    }

    // 6. 排序假設檢驗
    let matched: Vec<&str> = mapping
        .iter()
        .map(|m| {
            m.as_deref()
                .unwrap_or("")
                .split(" (近似")
                .next()
                .unwrap_or("")
        })
        .collect();
    let mut alpha = canvas_dfs.clone();
    alpha.sort();
    println!("\ncanvas 字母序 vs rect 序：");
    let rect_order_match_alpha = matched
        .iter()
        .zip(alpha.iter())
        .filter(|(a, b)| a == b)
        .count();
    println!("  字母序一致位置：{rect_order_match_alpha}/{}", alpha.len());
    let rect_order_match_dfs = matched
        .iter()
        .zip(canvas_dfs.iter())
        .filter(|(a, b)| a == b)
        .count();
    println!(
        "  DFS 序一致位置：{rect_order_match_dfs}/{}",
        canvas_dfs.len()
    );
    let unmatched: Vec<&String> = canvas_dfs
        .iter()
        .filter(|c| !matched.contains(&c.as_str()))
        .collect();
    println!("  無 rect 的 canvas：{unmatched:?}");
}

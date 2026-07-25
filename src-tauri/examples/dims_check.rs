//! 驗證「pool-B 字母序 + skip」假設：rect 尺寸序列是否等於
//! v83 canvas（字母序）尺寸序列跳過一個元素。
//! 用法：`cargo run --release --example dims_check -- <v83 Character.wz> <wzss bin>`

use maple_lib::wz::container::file::WzFile;
use maple_lib::wz::property::WzImageProperty;
use maple_lib::wz::types::WzMapleVersion;
use msclassic::image::parse_wzss;

fn collect(prop: &WzImageProperty, path: &str, out: &mut Vec<(String, i32, i32)>) {
    if let WzImageProperty::Canvas(c) = prop {
        out.push((path.to_string(), c.png.width, c.png.height));
    }
    if let Some(children) = prop.children() {
        for ch in children {
            collect(ch, &format!("{path}/{}", ch.name()), out);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut f = WzFile::open(args[0].clone(), WzMapleVersion::Gms);
    f.parse_wz_file().unwrap();
    let root = f.wz_directory_mut().unwrap();
    let img = root
        .get_directory_mut("Face")
        .and_then(|d| d.get_image_mut("00020116.img"))
        .unwrap();
    img.parse_image().unwrap();
    let mut v83: Vec<(String, i32, i32)> = Vec::new();
    for p in &img.properties {
        collect(p, p.name(), &mut v83);
    }
    v83.sort();

    let wzss = parse_wzss(&std::fs::read(&args[1]).unwrap()).unwrap();
    let rdims: Vec<(i32, i32)> = wzss
        .rects
        .iter()
        .map(|r| (r.w as i32, r.h as i32))
        .collect();

    println!("v83 canvas（字母序）{} 個：", v83.len());
    for (i, (p, w, h)) in v83.iter().enumerate() {
        println!("  [{i:2}] {p} {w}x{h}");
    }
    println!("rect 序 {} 個：{:?}", rdims.len(), rdims);

    // 測試：跳過第 k 個 v83 元素後，尺寸序列是否逐項相等
    for skip in 0..v83.len() {
        let seq: Vec<(i32, i32)> = v83
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, (_, w, h))| (*w, *h))
            .collect();
        if seq == rdims {
            println!(
                "\n✓ 命中：跳過 v83[{skip}] = {} 後尺寸序列完全一致",
                v83[skip].0
            );
            return;
        }
    }
    // 部分比對：最長前綴
    let pre = rdims
        .iter()
        .zip(v83.iter())
        .take_while(|(r, (_, w, h))| **r == (*w, *h))
        .count();
    println!("\n無完美 skip 命中；不跳過的前綴匹配長度 = {pre}");
}

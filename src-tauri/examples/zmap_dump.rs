//! 從 v83 Base.wz 抽出 zmap.img 的圖層名順序（給紙娃娃內嵌排序表用）。
//!
//! 用法：
//! ```text
//! cargo run --release --bin zmap_dump -- <Base.wz 路徑>
//! ```

use maple_lib::wz::container::file::WzFile;
use maple_lib::wz::property::WzImageProperty;
use maple_lib::wz::types::WzMapleVersion;

fn walk(prop: &WzImageProperty, depth: usize, out: &mut Vec<String>) {
    if depth == 1 {
        out.push(prop.name().to_string());
    }
    if let Some(children) = prop.children() {
        for ch in children {
            walk(ch, depth + 1, out);
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("用法: zmap_dump <Base.wz>");
    let mut f = WzFile::open(path, WzMapleVersion::Gms);
    f.parse_wz_file().expect("開啟失敗");
    let root = f.wz_directory_mut().unwrap();
    let img = root
        .get_image_mut("zmap.img")
        .expect("zmap.img 不存在");
    img.parse_image().expect("parse 失敗");
    let mut names = Vec::new();
    for p in &img.properties {
        walk(p, 0, &mut names);
    }
    println!("共 {} 層:", names.len());
    for n in &names {
        println!("{n}");
    }
}

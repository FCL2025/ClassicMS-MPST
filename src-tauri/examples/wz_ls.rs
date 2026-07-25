//! 列出 v83 WZ 頂層目錄結構（偵錯用）。
//! 用法: wz_ls <input.wz> [目錄路徑 如 Face]

use maple_lib::wz::container::file::WzFile;
use maple_lib::wz::types::WzMapleVersion;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法: wz_ls <input.wz> [dir1/dir2/...]");
        std::process::exit(2);
    }
    let mut f = WzFile::open(args[0].clone(), WzMapleVersion::Gms);
    f.parse_wz_file().expect("parse failed");
    let mut dir = f.wz_directory_mut().expect("no root");
    if let Some(path) = args.get(1) {
        for seg in path.split('/') {
            dir = dir
                .get_directory_mut(seg)
                .unwrap_or_else(|| panic!("目錄不存在: {seg}"));
        }
    }
    println!("目錄 {}:", args.get(1).map(|s| s.as_str()).unwrap_or("/"));
    for d in &dir.sub_dirs {
        println!("  [D] {}", d.name);
    }
    for i in &dir.images {
        println!("  [I] {}", i.name);
    }
}

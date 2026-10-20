//! WZJS 解碼驗證工具。
//!
//! 用法：
//! ```text
//! cargo run --release --example wzjs_dump -- <payload.bin>        # 解成樹印出 JSON
//! cargo run --release --example wzjs_dump -- --all <目錄>          # 全量解碼驗證 + 統計
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use msclassic::wzjs::{parse, WzNode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("用法: wzjs_dump <payload.bin> | --all <目錄>");
        std::process::exit(1);
    }
    if args[0] == "--all" {
        scan_all(Path::new(&args[1]));
    } else {
        let data = std::fs::read(&args[0])?;
        let root = parse(&data)?;
        println!("{}", serde_json::to_string_pretty(&root)?);
    }
    Ok(())
}

fn scan_all(dir: &Path) {
    let mut files: Vec<_> = walkdir_bins(dir);
    files.sort();
    let total_files = files.len();

    let mut ok = 0usize;
    let mut skipped = 0usize;
    let mut failed: Vec<(std::path::PathBuf, String)> = Vec::new();
    let mut total_nodes = 0usize;
    let mut type_dist: BTreeMap<i32, usize> = BTreeMap::new();
    let mut value_dist: BTreeMap<&'static str, usize> = BTreeMap::new();

    let t0 = std::time::Instant::now();
    for path in &files {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                failed.push((path.clone(), format!("讀檔失敗: {e}")));
                continue;
            }
        };
        // 非 WZJS payload（prefab / spritesheet 索引等）不算失敗
        if data.len() < 152 || &data[144..148] != b"WZJS" {
            skipped += 1;
            continue;
        }
        match parse(&data) {
            Ok(root) => {
                ok += 1;
                collect_stats(&root, &mut total_nodes, &mut type_dist, &mut value_dist);
            }
            Err(e) => failed.push((path.clone(), e.to_string())),
        }
    }
    let elapsed = t0.elapsed();

    println!("=== WZJS 全量解碼統計 ===");
    println!(
        "掃描 .bin: {total_files}（WZJS payload: {}，非 WZJS 跳過: {skipped}）",
        ok + failed.len()
    );
    println!("成功: {ok}，失敗: {}", failed.len());
    println!("總節點數: {total_nodes}，耗時 {:.2?}", elapsed);
    println!("\ntype tag 分佈:");
    for (t, c) in &type_dist {
        println!("  type {t:2}: {c}");
    }
    println!("\n值型別分佈:");
    for (v, c) in &value_dist {
        println!("  {v}: {c}");
    }
    if !failed.is_empty() {
        println!("\n失敗樣本（前 20）:");
        for (p, e) in failed.iter().take(20) {
            println!("  {}: {e}", p.display());
        }
    }
}

fn collect_stats(
    node: &WzNode,
    total: &mut usize,
    type_dist: &mut BTreeMap<i32, usize>,
    value_dist: &mut BTreeMap<&'static str, usize>,
) {
    *total += 1;
    *type_dist.entry(node.type_tag).or_default() += 1;
    let kind = match &node.value {
        msclassic::wzjs::Value::SubTree => "SubTree",
        msclassic::wzjs::Value::Canvas => "Canvas",
        msclassic::wzjs::Value::U8(_) => "U8",
        msclassic::wzjs::Value::U16(_) => "U16",
        msclassic::wzjs::Value::Int(_) => "Int",
        msclassic::wzjs::Value::Long(_) => "Long",
        msclassic::wzjs::Value::Float2(_, _) => "Float2",
        msclassic::wzjs::Value::Int2(_, _) => "Int2",
        msclassic::wzjs::Value::Double(_) => "Double",
        msclassic::wzjs::Value::Vector(_, _) => "Vector",
        msclassic::wzjs::Value::Str(_) => "Str",
        msclassic::wzjs::Value::Uol(_) => "Uol",
    };
    *value_dist.entry(kind).or_default() += 1;
    for c in &node.children {
        collect_stats(c, total, type_dist, value_dist);
    }
}

fn walkdir_bins(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "bin") {
                    out.push(p);
                }
            }
        }
    }
    out
}

//! WZSS 早期偵查工具：以「6 個 section triple」的假設列印 header。
//!
//! **注意**：那個假設已被第三輪逆向推翻（真正佈局見 `src/image.rs` 模組文件），
//! 本工具只保留當初的原始 hexdump 視角；要看正確分段請用 `wzss_pool`。
//! 用法：`cargo run --release --example wzss_probe -- [path 過濾字串]`

use msclassic::image::parse_wzss;

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default();
    let root = std::path::Path::new("out/mono");
    let mut entries: Vec<(String, String)> = Vec::new(); // (file path, wzss path)
    let mut errors: Vec<(String, String)> = Vec::new();
    for bundle in std::fs::read_dir(root).unwrap() {
        let bundle = bundle.unwrap().path();
        if !bundle.is_dir()
            || !bundle
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("spritesheet_")
        {
            continue;
        }
        for f in std::fs::read_dir(&bundle).unwrap() {
            let f = f.unwrap().path();
            if f.extension().is_none_or(|e| e != "bin") {
                continue;
            }
            let data = match std::fs::read(&f) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if data.len() < 80 {
                continue;
            }
            match parse_wzss(&data) {
                Ok(w) => {
                    entries.push((f.display().to_string(), w.path.clone()));
                    if filter.is_empty() || w.path.contains(&filter) {
                        dump_one(&data, &f.display().to_string());
                    }
                }
                Err(error) => errors.push((f.display().to_string(), error.to_string())),
            }
        }
    }
    if !filter.is_empty() {
        println!(
            "\n符合 '{filter}' 的 WZSS 共 {} 筆",
            entries.iter().filter(|(_, p)| p.contains(&filter)).count()
        );
    } else {
        println!("共 {} 個 WZSS", entries.len());
    }
    println!("解析失敗 {} 個", errors.len());
    for (path, error) in errors.iter().take(10) {
        println!("  {path}: {error}");
    }
}

fn dump_one(data: &[u8], file: &str) {
    let w = parse_wzss(data).unwrap();
    let magic = data
        .windows(4)
        .take(1024)
        .position(|x| x == b"WZSS")
        .unwrap();
    println!("=== {file}");
    println!(
        "path={} magic@{magic} len={} rects={}",
        w.path,
        data.len(),
        w.rects.len()
    );
    let u32_at = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
    println!("version(u32[0])={}", u32_at(0));
    for s in 0..6 {
        let (c, d, t) = (u32_at(1 + s * 3), u32_at(2 + s * 3), u32_at(3 + s * 3));
        println!("  section{s}: count={c} dataOff={d} tableOff={t}");
        if c > 0 && c < 100000 {
            let base = magic + d as usize;
            // 印前 4 筆（每筆先假設 16 bytes）
            for i in 0..(c.min(4)) as usize {
                let p = base + i * 16;
                if p + 16 <= data.len() {
                    let v: Vec<u32> = (0..4)
                        .map(|k| {
                            u32::from_le_bytes(data[p + k * 4..p + k * 4 + 4].try_into().unwrap())
                        })
                        .collect();
                    println!("    [{i}] {v:?}");
                }
            }
            // table 區
            let tb = magic + t as usize;
            if tb + 16 <= data.len() {
                let v: Vec<u32> = (0..4)
                    .map(|k| {
                        u32::from_le_bytes(data[tb + k * 4..tb + k * 4 + 4].try_into().unwrap())
                    })
                    .collect();
                println!("    table[0..4] {v:?}");
            }
        }
    }
    // rect 樣本
    for (i, r) in w.rects.iter().take(4).enumerate() {
        println!("  rect[{i}] x={} y={} w={} h={}", r.x, r.y, r.w, r.h);
    }
}

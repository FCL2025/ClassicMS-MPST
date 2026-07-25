//! type 8 值全量普查：統計 2×u32 當 f32 讀時「像 float」還是「像 int bits」，
//! 並列出 int-like 節點的名稱分佈，判斷 type 8 是否其實是「依節點而定的數值對」。

use std::collections::BTreeMap;

use msclassic::wzjs::{self, Value, WzNode};

fn walk(n: &WzNode, stats: &mut Stats) {
    let (t8_float, t8_int) = match n.value {
        Value::Float2(a, b) => (Some((a, b)), None),
        Value::Int2(a, b) => (None, Some((a, b))),
        _ => (None, None),
    };
    if let Some((a, b)) = t8_int {
        stats.int_like += 1;
        *stats.int_names.entry(n.name.clone()).or_insert(0) += 1;
        if stats.int_samples.len() < 10 {
            stats
                .int_samples
                .push(format!("{nname} = ({a}, {b})", nname = n.name));
        }
    } else if let Some((a, b)) = t8_float {
        let like_int = |f: f32| {
            let bits = f.to_bits();
            // denormal（指數全 0）且非零 → 幾乎可以確定是 int bits
            bits != 0 && (bits & 0x7F80_0000) == 0
        };
        if like_int(a) || like_int(b) {
            stats.int_like += 1;
            *stats.int_names.entry(n.name.clone()).or_insert(0) += 1;
        } else {
            stats.float_like += 1;
            *stats.float_names.entry(n.name.clone()).or_insert(0) += 1;
        }
    }
    for c in &n.children {
        walk(c, stats);
    }
}

#[derive(Default)]
struct Stats {
    float_like: usize,
    int_like: usize,
    float_names: BTreeMap<String, usize>,
    int_names: BTreeMap<String, usize>,
    int_samples: Vec<String>,
}

fn main() {
    let root = std::path::Path::new("out/mono");
    let mut stats = Stats::default();
    let mut files = 0usize;
    let mut seen = 0usize;
    let mut magic_miss = 0usize;
    let mut parse_fail = 0usize;
    for bundle in std::fs::read_dir(root).unwrap() {
        let bundle = bundle.unwrap().path();
        if !bundle.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&bundle).unwrap() {
            let f = f.unwrap().path();
            if f.extension().is_none_or(|e| e != "bin") {
                continue;
            }
            seen += 1;
            let data = std::fs::read(&f).unwrap();
            // .bin 直接就是自訂 payload（無 Unity 標頭），magic 在 offset 144
            if data.len() < 148 || data.get(144..148) != Some(b"WZJS") {
                magic_miss += 1;
                continue;
            }
            match wzjs::parse(&data) {
                Ok(tree) => {
                    files += 1;
                    walk(&tree, &mut stats);
                }
                Err(_) => parse_fail += 1,
            }
        }
    }
    eprintln!("bin 檔 {seen} / magic_miss {magic_miss} / parse_fail {parse_fail}");
    println!(
        "掃描 {files} 個 WZJS：type 8 共 {} 筆",
        stats.float_like + stats.int_like
    );
    println!("  像 float：{}", stats.float_like);
    println!("  像 int bits：{}", stats.int_like);
    println!("\n像 int 的節點名稱分佈（前 20）：");
    let mut v: Vec<_> = stats.int_names.iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (name, c) in v.iter().take(20) {
        println!("  {name}: {c}");
    }
    println!("\n像 float 的節點名稱分佈（前 20）：");
    let mut v: Vec<_> = stats.float_names.iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (name, c) in v.iter().take(20) {
        println!("  {name}: {c}");
    }
    println!("\nint-like 樣本：");
    for s in &stats.int_samples {
        println!("  {s}");
    }
}

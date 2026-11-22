//! WZSS 節點雜湊池調查（第三輪）。
//!
//! 解出的 header（20 × u32）：
//! ```text
//! [0]  version
//! [1]  常數 8
//! [2..5)   UOL 字串池 (count, blobOff, tableOff)
//! [5..7)   ? (count, off)  8B/筆
//! [7..9)   sprite rect (count, off) 16B/筆
//! [9..11)  frame (count, off) 20B/筆
//! [11..13) uol link (count, off) 16B/筆
//! [13..16) 32-hex 池 (count, blobOff, tableOff) — count = canvas + uol 節點數
//! [16..19) 64-hex 池 (count, blobOff, tableOff)
//! [19] path 長度
//! ```
//!
//! 用法：
//! - `... --example wzss_pool -- dump <sheet key>`
//! - `... --example wzss_pool -- xcheck <key a> <key b>`
//! - `... --example wzss_pool -- stats [上限]`

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use msclassic::catalog::DataService;
use msclassic::unity::objects;
use msclassic::wzjs::{Value, WzNode};

const DEFAULT_DATA_DIR: &str =
    r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w";

struct Wzss {
    path: String,
    rects: Vec<[u32; 4]>,
    frames: Vec<[i32; 5]>,
    links: Vec<[i32; 4]>,
    uol_strings: Vec<String>,
    hash32: Vec<String>,
    hash64: Vec<String>,
    sec5: Vec<[i32; 2]>,
}

fn parse(data: &[u8]) -> Option<Wzss> {
    let magic = data.windows(4).position(|window| window == b"WZSS")?;
    fn read_u32(data: &[u8], pos: usize) -> Option<u32> {
        Some(u32::from_le_bytes(data.get(pos..pos + 4)?.try_into().ok()?))
    }
    let head = |index: usize| read_u32(data, index * 4);

    let path_len = head(19)? as usize;
    let path = String::from_utf8_lossy(data.get(80..80 + path_len)?).into_owned();

    let read_records = |count: usize, off: usize, stride: usize| -> Option<Vec<Vec<i32>>> {
        let start = magic + off;
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            let pos = start + index * stride;
            let mut record = Vec::with_capacity(stride / 4);
            for word in 0..stride / 4 {
                record.push(read_u32(data, pos + word * 4)? as i32);
            }
            out.push(record);
        }
        Some(out)
    };

    let read_pool = |count: usize, blob: usize, table: usize| -> Option<Vec<String>> {
        let blob = magic + blob;
        let table = magic + table;
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            let start = read_u32(data, table + index * 4)? as usize;
            let end = read_u32(data, table + index * 4 + 4)? as usize;
            out.push(String::from_utf8_lossy(data.get(blob + start..blob + end)?).into_owned());
        }
        Some(out)
    };

    let rects = read_records(head(7)? as usize, head(8)? as usize, 16)?
        .into_iter()
        .map(|record| {
            [
                record[0] as u32,
                record[1] as u32,
                record[2] as u32,
                record[3] as u32,
            ]
        })
        .collect();
    let frames = read_records(head(9)? as usize, head(10)? as usize, 20)?
        .into_iter()
        .map(|record| [record[0], record[1], record[2], record[3], record[4]])
        .collect();
    let links = read_records(head(11)? as usize, head(12)? as usize, 16)?
        .into_iter()
        .map(|record| [record[0], record[1], record[2], record[3]])
        .collect();
    let sec5 = read_records(head(5)? as usize, head(6)? as usize, 8)?
        .into_iter()
        .map(|record| [record[0], record[1]])
        .collect();
    let uol_strings = read_pool(head(2)? as usize, head(3)? as usize, head(4)? as usize)?;
    let hash32 = read_pool(head(13)? as usize, head(14)? as usize, head(15)? as usize)?;
    let hash64 = read_pool(head(16)? as usize, head(17)? as usize, head(18)? as usize)?;
    Some(Wzss {
        path,
        rects,
        frames,
        links,
        uol_strings,
        hash32,
        hash64,
        sec5,
    })
}

fn load(svc: &DataService, key: &str) -> Option<Wzss> {
    let entry = svc.sheet(key)?;
    let payload = svc.read_entry(entry.index_loc.as_ref()?).ok()?;
    let mono = objects::parse_mono_behaviour(&payload, true).ok()?;
    parse(payload.get(mono.header_len..)?)
}

/// DFS 收集 canvas 與 uol 節點（相對 img 根的路徑）。
fn collect(node: &WzNode, path: &str, canvas: &mut Vec<String>, uol: &mut Vec<(String, String)>) {
    for child in &node.children {
        let here = if path.is_empty() {
            child.name.clone()
        } else {
            format!("{path}/{}", child.name)
        };
        match &child.value {
            Value::Canvas => canvas.push(here.clone()),
            Value::Uol(target) => uol.push((here.clone(), target.clone())),
            _ => {}
        }
        collect(child, &here, canvas, uol);
    }
}

fn open() -> DataService {
    DataService::open(
        Path::new(DEFAULT_DATA_DIR),
        Path::new("out/probe_cache.json"),
    )
    .unwrap()
    .0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("dump") => dump(&args[1]),
        Some("xcheck") => xcheck(&args[1], &args[2]),
        Some("stats") => stats(args.get(1).and_then(|n| n.parse().ok())),
        Some("pairs") => pairs(args.get(1).and_then(|n| n.parse().ok())),
        Some("poolb") => poolb(&args[1]),
        _ => println!("用法: wzss_pool dump|xcheck|stats ..."),
    }
}

fn dump(key: &str) {
    let svc = open();
    let Some(wzss) = load(&svc, key) else {
        println!("找不到或無法解析：{key}");
        return;
    };
    let tree = svc.tree_for(key).unwrap();
    let (mut canvas, mut uol) = (Vec::new(), Vec::new());
    collect(&tree, "", &mut canvas, &mut uol);

    println!("== {key} (wzss path={}) ==", wzss.path);
    println!(
        "rects={} frames={} links={} uol字串={} hash32={} hash64={} sec5={}",
        wzss.rects.len(),
        wzss.frames.len(),
        wzss.links.len(),
        wzss.uol_strings.len(),
        wzss.hash32.len(),
        wzss.hash64.len(),
        wzss.sec5.len(),
    );
    println!("WZJS: canvas={} uol={}", canvas.len(), uol.len());
    println!("sec5: {:?}", wzss.sec5);
    println!(
        "hash32[0] len={}",
        wzss.hash32.first().map_or(0, |s| s.len())
    );
    println!(
        "hash64[0] len={}",
        wzss.hash64.first().map_or(0, |s| s.len())
    );

    let by_node: HashMap<i32, &[i32; 4]> = wzss.links.iter().map(|link| (link[0], link)).collect();
    println!("\n-- hash32 池（節點槽）--");
    for (index, hash) in wzss.hash32.iter().enumerate() {
        let mut note = String::new();
        if let Some(link) = by_node.get(&(index as i32)) {
            let target = wzss
                .uol_strings
                .get(link[1] as usize)
                .cloned()
                .unwrap_or_default();
            let rect = wzss
                .frames
                .get(link[3] as usize)
                .and_then(|frame| wzss.rects.get(frame[3] as usize))
                .map(|rect| format!("{}x{}", rect[2], rect[3]))
                .unwrap_or_else(|| "空".into());
            note = format!("UOL→{target} frame={} rect={rect} f2={}", link[3], link[2]);
        }
        println!("  [{index:3}] {hash} {note}");
    }
    println!("\n-- frames --");
    for (index, frame) in wzss.frames.iter().enumerate() {
        let rect = wzss.rects.get(frame[3] as usize);
        println!(
            "  [{index:3}] {frame:?} rect={:?}",
            rect.map(|r| format!("{}x{} @({},{})", r[2], r[3], r[0], r[1]))
        );
    }
    println!("\n-- hash64 池 --");
    for (index, hash) in wzss.hash64.iter().enumerate() {
        println!("  [{index:3}] {hash}");
    }
    println!("\n-- WZJS canvas（DFS）--");
    for (index, path) in canvas.iter().enumerate() {
        println!("  [{index:3}] {path}");
    }
    println!("\n-- WZJS uol（DFS，前 20）--");
    for (index, (path, target)) in uol.iter().enumerate().take(20) {
        println!("  [{index:3}] {path} → {target}");
    }
}

fn xcheck(a: &str, b: &str) {
    let svc = open();
    let (Some(wa), Some(wb)) = (load(&svc, a), load(&svc, b)) else {
        println!("載入失敗");
        return;
    };
    let set_a: BTreeMap<&str, usize> = wa
        .hash32
        .iter()
        .enumerate()
        .map(|(index, hash)| (hash.as_str(), index))
        .collect();
    let common: Vec<(&str, usize, usize)> = wb
        .hash32
        .iter()
        .enumerate()
        .filter_map(|(index, hash)| {
            set_a
                .get(hash.as_str())
                .map(|other| (hash.as_str(), *other, index))
        })
        .collect();
    println!("{a}: hash32={} rects={}", wa.hash32.len(), wa.rects.len());
    println!("{b}: hash32={} rects={}", wb.hash32.len(), wb.rects.len());
    println!("共同 hash32：{}", common.len());
    for (hash, ia, ib) in common.iter().take(20) {
        println!("  {hash} a[{ia}] b[{ib}]");
    }
    let set64: BTreeMap<&str, usize> = wa
        .hash64
        .iter()
        .enumerate()
        .map(|(index, hash)| (hash.as_str(), index))
        .collect();
    let common64 = wb
        .hash64
        .iter()
        .filter(|hash| set64.contains_key(hash.as_str()))
        .count();
    println!("共同 hash64：{common64}");
}

/// 蒐集「路徑 → hash32」確定樣本：
/// - canvas 只有 1 個時，pool[0] 必定是它。
/// - uol 只有 1 個時，pool[canvas 數] 必定是它。
///   輸出到 out/wzss_pairs.txt，供雜湊函式爆破使用。
fn pairs(limit: Option<usize>) {
    let svc = open();
    let keys: Vec<String> = svc.sheets().map(|(key, _)| key).cloned().collect();
    let mut out = String::new();
    let mut count = 0usize;
    for key in keys.iter().take(limit.unwrap_or(usize::MAX)) {
        let Some(wzss) = load(&svc, key) else {
            continue;
        };
        let Ok(tree) = svc.tree_for(key) else {
            continue;
        };
        let (mut canvas, mut uol) = (Vec::new(), Vec::new());
        collect(&tree, "", &mut canvas, &mut uol);
        if wzss.hash32.len() != canvas.len() + uol.len() {
            continue;
        }
        if canvas.len() == 1 {
            out.push_str(&format!(
                "canvas\t{}\t{}\t{}\n",
                wzss.path, canvas[0], wzss.hash32[0]
            ));
            count += 1;
        }
        if uol.len() == 1 {
            out.push_str(&format!(
                "uol\t{}\t{}\t{}\n",
                wzss.path,
                uol[0].0,
                wzss.hash32[canvas.len()]
            ));
            count += 1;
        }
    }
    std::fs::create_dir_all("out").ok();
    std::fs::write("out/wzss_pairs.txt", &out).unwrap();
    println!("寫出 {count} 組確定樣本到 out/wzss_pairs.txt");
    for line in out.lines().take(10) {
        println!("  {line}");
    }
}

/// 印出 WZJS 節點的 pool B 路徑字串（canvas / uol 節點）。
fn poolb(key: &str) {
    let svc = open();
    let tree = svc.tree_for(key).unwrap();
    fn walk(node: &WzNode, depth: usize, shown: &mut usize) {
        if *shown > 40 {
            return;
        }
        let kind = match &node.value {
            Value::Canvas => "canvas",
            Value::Uol(_) => "uol",
            Value::SubTree => "sub",
            _ => "val",
        };
        println!(
            "{:indent$}{kind} name={:?} path={:?} tag={}",
            "",
            node.name,
            node.path,
            node.type_tag,
            indent = depth * 2
        );
        *shown += 1;
        for child in &node.children {
            walk(child, depth + 1, shown);
        }
    }
    let mut shown = 0;
    walk(&tree, 0, &mut shown);
}

fn stats(limit: Option<usize>) {
    let svc = open();
    let keys: Vec<String> = svc.sheets().map(|(key, _)| key).cloned().collect();
    let mut checked = 0usize;
    let mut ok_counts = 0usize;
    let mut bad_counts = Vec::new();
    let mut link_eq_uol = 0usize;
    let mut hash32_len_bad = 0usize;
    let mut hash64_vs_rect = 0usize;
    let mut hash64_vs_canvas = 0usize;
    for key in keys.iter().take(limit.unwrap_or(usize::MAX)) {
        let Some(wzss) = load(&svc, key) else {
            continue;
        };
        let Ok(tree) = svc.tree_for(key) else {
            continue;
        };
        let (mut canvas, mut uol) = (Vec::new(), Vec::new());
        collect(&tree, "", &mut canvas, &mut uol);
        checked += 1;
        if wzss.hash32.len() == canvas.len() + uol.len() {
            ok_counts += 1;
        } else if bad_counts.len() < 15 {
            bad_counts.push(format!(
                "{key}: hash32={} canvas={} uol={} rects={} links={} frames={}",
                wzss.hash32.len(),
                canvas.len(),
                uol.len(),
                wzss.rects.len(),
                wzss.links.len(),
                wzss.frames.len()
            ));
        }
        if wzss.links.len() == uol.len() {
            link_eq_uol += 1;
        }
        if wzss.hash32.iter().any(|hash| hash.len() != 32) {
            hash32_len_bad += 1;
        }
        if wzss.hash64.len() == wzss.rects.len() {
            hash64_vs_rect += 1;
        }
        if wzss.hash64.len() == canvas.len() {
            hash64_vs_canvas += 1;
        }
    }
    println!("檢查 {checked} 個 sheet");
    println!("  hash32 數 == canvas+uol：{ok_counts}");
    println!("  links 數 == uol：{link_eq_uol}");
    println!("  hash64 數 == rect：{hash64_vs_rect}");
    println!("  hash64 數 == canvas：{hash64_vs_canvas}");
    println!("  hash32 長度非 32：{hash32_len_bad}");
    for line in bad_counts {
        println!("  !! {line}");
    }
}

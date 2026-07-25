//! WZSS canvas→rect 映射全量驗證（第三輪定案）。
//!
//! 假說：WZSS 的 32-hex 節點鍵 =
//! `Guid(MD5(UTF8("<img 路徑>/<節點相對路徑>"))).ToString("N")`
//! （.NET Guid 位元組序：前 4 bytes 反轉、接著 2、2 反轉，其餘原序）。
//!
//! 池的佈局：前 `frames.len()` 個槽是 item（槽 i ↔ frame i），
//! 其後是 link（別名：UOL 或重複圖），由 link 記錄的 `[0]` 指回槽號。
//!
//! 用法：`cargo run --release --features research-tools --example wzss_verify -- [上限]`

use std::collections::HashMap;
use std::path::Path;

use msclassic::catalog::DataService;
use msclassic::unity::objects;
use msclassic::wzjs::{Value, WzNode};

const DEFAULT_DATA_DIR: &str =
    r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w";

/// `Guid(md5).ToString("N")`：前 4 bytes 反轉、次 2 bytes 反轉、再 2 bytes 反轉，其餘原序。
fn node_key(full_path: &str) -> String {
    let digest = md5::compute(full_path.as_bytes());
    let bytes = digest.0;
    let order = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];
    order
        .iter()
        .map(|index| format!("{:02x}", bytes[*index]))
        .collect()
}

struct Wzss {
    path: String,
    rects: Vec<[u32; 4]>,
    frames: Vec<[i32; 5]>,
    links: Vec<[i32; 4]>,
    hash32: Vec<String>,
}

fn parse(data: &[u8]) -> Option<Wzss> {
    let magic = data.windows(4).position(|window| window == b"WZSS")?;
    fn read_u32(data: &[u8], pos: usize) -> Option<u32> {
        Some(u32::from_le_bytes(data.get(pos..pos + 4)?.try_into().ok()?))
    }
    let head = |index: usize| read_u32(data, index * 4);
    let path_len = head(19)? as usize;
    let path = String::from_utf8_lossy(data.get(80..80 + path_len)?).into_owned();

    let records = |count: usize, off: usize, stride: usize| -> Option<Vec<Vec<i32>>> {
        (0..count)
            .map(|index| {
                let pos = magic + off + index * stride;
                (0..stride / 4)
                    .map(|word| read_u32(data, pos + word * 4).map(|value| value as i32))
                    .collect::<Option<Vec<i32>>>()
            })
            .collect()
    };
    let pool = |count: usize, blob: usize, table: usize| -> Option<Vec<String>> {
        (0..count)
            .map(|index| {
                let start = read_u32(data, magic + table + index * 4)? as usize;
                let end = read_u32(data, magic + table + index * 4 + 4)? as usize;
                Some(
                    String::from_utf8_lossy(data.get(magic + blob + start..magic + blob + end)?)
                        .into_owned(),
                )
            })
            .collect()
    };

    Some(Wzss {
        path,
        rects: records(head(7)? as usize, head(8)? as usize, 16)?
            .into_iter()
            .map(|r| [r[0] as u32, r[1] as u32, r[2] as u32, r[3] as u32])
            .collect(),
        frames: records(head(9)? as usize, head(10)? as usize, 20)?
            .into_iter()
            .map(|r| [r[0], r[1], r[2], r[3], r[4]])
            .collect(),
        links: records(head(11)? as usize, head(12)? as usize, 16)?
            .into_iter()
            .map(|r| [r[0], r[1], r[2], r[3]])
            .collect(),
        hash32: pool(head(13)? as usize, head(14)? as usize, head(15)? as usize)?,
    })
}

fn collect(node: &WzNode, path: &str, canvas: &mut Vec<String>, uol: &mut Vec<String>) {
    for child in &node.children {
        let here = if path.is_empty() {
            child.name.clone()
        } else {
            format!("{path}/{}", child.name)
        };
        match &child.value {
            Value::Canvas => canvas.push(here.clone()),
            Value::Uol(_) => uol.push(here.clone()),
            _ => {}
        }
        collect(child, &here, canvas, uol);
    }
}

fn main() {
    let limit: usize = std::env::args()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(usize::MAX);
    let (svc, _) = DataService::open(
        Path::new(DEFAULT_DATA_DIR),
        Path::new("out/probe_cache.json"),
    )
    .unwrap();

    let keys: Vec<String> = svc.catalog.image.sheets.keys().cloned().collect();
    let (mut sheets, mut no_tree, mut parse_fail) = (0usize, 0usize, 0usize);
    let (mut key_all_found, mut key_partial) = (0usize, 0usize);
    let (mut canvas_total, mut canvas_hit, mut canvas_resolved) = (0usize, 0usize, 0usize);
    let (mut uol_total, mut uol_hit) = (0usize, 0usize);
    let mut frame_identity_bad = 0usize;
    let mut samples: Vec<String> = Vec::new();

    for key in keys.iter().take(limit) {
        let Some(entry) = svc.catalog.image.sheets.get(key) else {
            continue;
        };
        let Some(index_loc) = entry.index_loc.as_ref() else {
            continue;
        };
        let Ok(payload) = svc.read_entry(index_loc) else {
            continue;
        };
        let Ok(mono) = objects::parse_mono_behaviour(&payload, true) else {
            continue;
        };
        let Some(wzss) = payload.get(mono.header_len..).and_then(parse) else {
            parse_fail += 1;
            continue;
        };
        let Ok(tree) = svc.tree_for(&wzss.path) else {
            no_tree += 1;
            continue;
        };
        sheets += 1;

        let (mut canvas, mut uol) = (Vec::new(), Vec::new());
        collect(&tree, "", &mut canvas, &mut uol);

        let slot: HashMap<&str, usize> = wzss
            .hash32
            .iter()
            .enumerate()
            .map(|(index, hash)| (hash.as_str(), index))
            .collect();
        let link_slot: HashMap<i32, &[i32; 4]> =
            wzss.links.iter().map(|link| (link[0], link)).collect();

        for (index, frame) in wzss.frames.iter().enumerate() {
            if frame[0] != index as i32 || frame[1] != index as i32 {
                frame_identity_bad += 1;
                println!(
                    "  ?? frame 非恆等：{} frame[{index}]={frame:?} frames={} rects={} 池={}",
                    wzss.path,
                    wzss.frames.len(),
                    wzss.rects.len(),
                    wzss.hash32.len()
                );
                break;
            }
        }

        let mut missing = 0usize;
        for path in &canvas {
            canvas_total += 1;
            let hash = node_key(&format!("{}/{}", wzss.path, path));
            match slot.get(hash.as_str()) {
                Some(index) => {
                    canvas_hit += 1;
                    // 槽 → frame → rect
                    let frame = if *index < wzss.frames.len() {
                        Some(wzss.frames[*index][3])
                    } else {
                        link_slot
                            .get(&(*index as i32))
                            .map(|link| link[3])
                            .filter(|f| (*f as usize) < wzss.frames.len())
                            .map(|f| wzss.frames[f as usize][3])
                    };
                    if frame.is_some_and(|rect| (rect as usize) < wzss.rects.len()) {
                        canvas_resolved += 1;
                    }
                }
                None => {
                    missing += 1;
                    if samples.len() < 12 {
                        samples.push(format!("{}/{path} → {hash} 不在池中", wzss.path));
                    }
                }
            }
        }
        for path in &uol {
            uol_total += 1;
            let hash = node_key(&format!("{}/{}", wzss.path, path));
            if slot.contains_key(hash.as_str()) {
                uol_hit += 1;
            }
        }
        if missing == 0 {
            key_all_found += 1;
        } else {
            key_partial += 1;
        }
    }

    println!("sheet 檢查數：{sheets}（無 tree {no_tree}、解析失敗 {parse_fail}）");
    println!("  全部 canvas 命中的 sheet：{key_all_found}");
    println!("  有 canvas 落空的 sheet：{key_partial}");
    println!(
        "  canvas 鍵命中：{canvas_hit}/{canvas_total}（{:.4}%）",
        canvas_hit as f64 * 100.0 / canvas_total.max(1) as f64
    );
    println!(
        "  canvas 可解出 rect：{canvas_resolved}/{canvas_total}（{:.4}%）",
        canvas_resolved as f64 * 100.0 / canvas_total.max(1) as f64
    );
    println!(
        "  uol 鍵命中：{uol_hit}/{uol_total}（{:.4}%）",
        uol_hit as f64 * 100.0 / uol_total.max(1) as f64
    );
    println!("  frames[i][0]/[1] != i 的 sheet：{frame_identity_bad}");
    for line in samples {
        println!("  !! {line}");
    }
}

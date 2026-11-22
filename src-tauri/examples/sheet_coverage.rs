//! 量化 spritesheet frame 映射覆蓋率：走正式 API（`parse_wzss` +
//! `WzssIndex::frame_index_for_node`）逐一檢查每個 canvas 節點能否定位到單一 rect。
//! 用法：`cargo run --release --features research-tools --example sheet_coverage`

use msclassic::catalog::DataService;
use msclassic::image::parse_wzss;
use msclassic::unity::objects;
use msclassic::wzjs::{Value, WzNode};

const DEFAULT_DATA_DIR: &str =
    r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w";

fn collect_canvas(node: &WzNode, path: &str, out: &mut Vec<String>) {
    for child in &node.children {
        let here = if path.is_empty() {
            child.name.clone()
        } else {
            format!("{path}/{}", child.name)
        };
        if matches!(child.value, Value::Canvas) {
            out.push(here.clone());
        }
        collect_canvas(child, &here, out);
    }
}

fn main() {
    let cache = std::path::Path::new("out/coverage_cache.json");
    let (svc, info) = DataService::open(std::path::Path::new(DEFAULT_DATA_DIR), cache).unwrap();
    eprintln!("索引 {} 筆（{}ms）", info.entry_count, info.elapsed_ms);

    let keys: Vec<String> = svc.sheets().map(|(key, _)| key).cloned().collect();
    let mut sheets = 0usize;
    let mut sheets_full = 0usize;
    let mut canvas_total = 0usize;
    let mut canvas_exact = 0usize;
    let mut misses: Vec<String> = Vec::new();
    for key in &keys {
        let entry = svc.sheet(key).expect("列出的圖集應存在");
        let Some(index_loc) = entry.index_loc.as_ref() else {
            continue;
        };
        let Ok(payload) = svc.read_entry(index_loc) else {
            continue;
        };
        let Ok(mono) = objects::parse_mono_behaviour(&payload, true) else {
            continue;
        };
        let Ok(wzss) = parse_wzss(&payload[mono.header_len..]) else {
            continue;
        };
        let Ok(tree) = svc.tree_for(&wzss.path) else {
            continue;
        };
        sheets += 1;
        let mut canvas = Vec::new();
        collect_canvas(&tree, "", &mut canvas);
        let mut missing = 0usize;
        for path in &canvas {
            canvas_total += 1;
            if wzss.frame_index_for_node(path).is_some() {
                canvas_exact += 1;
            } else {
                missing += 1;
                misses.push(format!("{}/{path}", wzss.path));
            }
        }
        if missing == 0 {
            sheets_full += 1;
        }
    }
    println!("sheet 總數：{sheets}");
    println!(
        "  全部 canvas 都能精準切圖的 sheet：{sheets_full}（{:.2}%）",
        sheets_full as f64 * 100.0 / sheets.max(1) as f64
    );
    println!(
        "  canvas 精準切圖：{canvas_exact}/{canvas_total}（{:.4}%）",
        canvas_exact as f64 * 100.0 / canvas_total.max(1) as f64
    );
    println!(
        "  無對應鍵（回退整頁 atlas）：{}",
        canvas_total - canvas_exact
    );
    let under_map = misses.iter().filter(|path| path.contains("/map/")).count();
    println!("    其中巢狀在 map 節點下：{under_map}");
    for line in misses
        .iter()
        .filter(|path| !path.contains("/map/"))
        .take(20)
    {
        println!("    其他：{line}");
    }
}

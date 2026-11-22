//! UOL 節點取圖覆蓋率：走與 GUI 完全相同的路徑（`get_image` → 失敗才
//! `get_canvas_atlas_fallback`），驗證開放 UOL 預覽後大量真實資料不會
//! panic、且大部分能精準切幀。
//! 用法：`cargo run --release --features research-tools --example uol_coverage -- [sheet 上限]`

use msclassic::catalog::DataService;
use msclassic::wzjs::{Value, WzNode};

const DEFAULT_DATA_DIR: &str =
    r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w";

fn collect_uol(node: &WzNode, path: &str, out: &mut Vec<String>) {
    for child in &node.children {
        let here = if path.is_empty() {
            child.name.clone()
        } else {
            format!("{path}/{}", child.name)
        };
        if matches!(child.value, Value::Uol(_)) {
            out.push(here.clone());
        }
        collect_uol(child, &here, out);
    }
}

fn main() {
    let limit: usize = std::env::args()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(3000);
    let (svc, info) = DataService::open(
        std::path::Path::new(DEFAULT_DATA_DIR),
        std::path::Path::new("out/probe_cache.json"),
    )
    .unwrap();
    eprintln!("索引 {} 筆（{}ms）", info.entry_count, info.elapsed_ms);

    let keys: Vec<String> = svc.sheets().map(|(key, _)| key).cloned().collect();
    let mut exact = 0usize;
    let mut atlas = 0usize;
    let mut none = 0usize;
    let mut hard_err = 0usize;
    let mut samples: Vec<String> = Vec::new();
    let mut node_total = 0usize;

    for key in keys.iter().take(limit) {
        let Ok(tree) = svc.tree_for(key) else {
            continue;
        };
        let mut uol = Vec::new();
        collect_uol(&tree, "", &mut uol);
        for rel in uol {
            let path = format!("{key}/{rel}");
            node_total += 1;
            match svc.get_image(&path) {
                Ok(Some(_)) => exact += 1,
                Ok(None) => none += 1,
                Err(_) => match svc.get_canvas_atlas_fallback(&path) {
                    Ok(Some(_)) => atlas += 1,
                    Ok(None) => none += 1,
                    Err(error) => {
                        hard_err += 1;
                        if samples.len() < 20 {
                            samples.push(format!("{path}: {error}"));
                        }
                    }
                },
            }
        }
    }

    println!(
        "UOL 節點總數：{node_total}（來自 {} 個 sheet）",
        limit.min(keys.len())
    );
    println!(
        "  精準切幀：{exact}（{:.4}%）",
        exact as f64 * 100.0 / node_total.max(1) as f64
    );
    println!("  atlas 回退：{atlas}");
    println!("  無圖片（空 sentinel 或查無）：{none}");
    println!("  兩條路徑都出錯：{hard_err}");
    for line in samples {
        println!("  !! {line}");
    }
}

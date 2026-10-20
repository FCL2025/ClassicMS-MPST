//! 雲端託管資料離線 pipeline（docs/HOSTED_DATA.md）。
//!
//! 輸入遊戲 bundle 目錄，產出託管用衍生資料集到 `<out_dir>/<version>/`：
//! `index.json.gz` / `catalog.json.gz` / `tree/**.json.gz` / `strings/**.json.gz` /
//! `sheets/**`（png）/ `textures/**`（png）/ `features/**.json.gz`
//! （聚合資料：怪物圖鑑、技能書、商城、紙娃娃、動作延遲）/ `audio/**`（Ogg）。
//! 所有 JSON 產出皆為 gzip 壓縮（`.json.gz`），不保留未壓縮版本。
//!
//! 用法：
//! ```text
//! cargo run -p msclassic --release --example hosted_export -- \
//!     <bundle 目錄> <輸出目錄> [--version vYYYYMMDD] [--limit-imgs N] [--limit-sheets N] \
//!     [--skip-trees] [--skip-strings] [--skip-sheets] [--skip-textures] [--skip-features] \
//!     [--skip-audio]
//! ```

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use msclassic::catalog::{DataService, EntryLocation};
use msclassic::image::{parse_sheet_asset, parse_wzss, SheetAsset, WzssIndex, WzssSprite};
use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};
use msclassic::unity::texture;
use msclassic::wzjs::node::is_canvas_tag;
use msclassic::wzjs::{Value, WzNode};

struct Args {
    bundle_dir: PathBuf,
    out_dir: PathBuf,
    version: String,
    generated_at: String,
    limit_imgs: Option<usize>,
    limit_sheets: Option<usize>,
    skip_trees: bool,
    skip_strings: bool,
    skip_sheets: bool,
    skip_textures: bool,
    skip_features: bool,
    skip_audio: bool,
}

#[derive(Default)]
struct Stats {
    imgs: usize,
    trees: usize,
    tree_fail: usize,
    strings: usize,
    string_fail: usize,
    sheets: usize,
    sheet_fail: usize,
    textures: usize,
    texture_fail: usize,
    canvas_total: usize,
    canvas_resolved: usize,
    features_ok: usize,
    features_fail: usize,
    feature_counts: serde_json::Map<String, serde_json::Value>,
    feature_bytes: serde_json::Map<String, serde_json::Value>,
    audio: usize,
    audio_fail: usize,
    bytes_audio: u64,
    bytes_catalog: u64,
    bytes_tree: u64,
    bytes_strings: u64,
    bytes_sheets: u64,
    bytes_textures: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let t0 = std::time::Instant::now();

    let out_root = args.out_dir.join(&args.version);
    let cache_dir = args.out_dir.join(".catalog-cache");
    std::fs::create_dir_all(&cache_dir)?;
    let (svc, info) = DataService::open(&args.bundle_dir, &cache_dir.join("catalog.json"))?;
    println!(
        "開啟: from_cache={} 耗時={}ms entries={} bundles={} sheets={}",
        info.from_cache,
        info.elapsed_ms,
        info.entry_count,
        info.bundle_count,
        svc.catalog.image.sheets.len()
    );

    // 驗證「img 邏輯路徑 == sheet key」的對應關係。
    let missing = svc
        .catalog
        .image
        .sheets
        .keys()
        .filter(|key| !svc.catalog.entries.contains_key(*key))
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        println!("!! 有 sheet key 不在 catalog.entries，樣本: {missing:?}");
    }

    let mut stats = Stats::default();

    // ── catalog.json.gz：精簡索引 ──
    std::fs::create_dir_all(&out_root)?;
    let entries: serde_json::Map<String, serde_json::Value> = svc
        .catalog
        .entries
        .iter()
        .map(|(path, loc)| (path.clone(), serde_json::json!(loc.class_id)))
        .collect();
    let catalog_json = serde_json::to_vec(&serde_json::json!({ "entries": entries }))?;
    stats.bytes_catalog = write_json_gz(&out_root.join("catalog.json"), &catalog_json)?;
    println!("catalog.json.gz: {} entries", svc.catalog.entries.len());

    if !args.skip_trees {
        export_trees(&svc, &out_root, args.limit_imgs, &mut stats)?;
    }
    if !args.skip_strings {
        export_strings(&svc, &out_root, &mut stats)?;
    }
    if !args.skip_sheets {
        export_sheets(&svc, &out_root, args.limit_sheets, &mut stats)?;
    }
    if !args.skip_textures {
        export_textures(&svc, &out_root, &mut stats)?;
    }
    if !args.skip_features {
        export_features(&svc, &out_root, &mut stats)?;
    }
    if !args.skip_audio {
        export_audio(&svc, &out_root, &mut stats)?;
    }

    // ── index.json.gz ──
    let index = serde_json::json!({
        "version": args.version,
        "generated_at": args.generated_at,
        "counts": {
            "imgs": stats.imgs,
            "trees": stats.trees,
            "strings": stats.strings,
            "sheets": stats.sheets,
            "textures": stats.textures,
            "canvas_total": stats.canvas_total,
            "canvas_resolved": stats.canvas_resolved,
            "features": stats.feature_counts,
            "audio": stats.audio,
        },
        "bytes": {
            "catalog": stats.bytes_catalog,
            "tree": stats.bytes_tree,
            "strings": stats.bytes_strings,
            "sheets": stats.bytes_sheets,
            "textures": stats.bytes_textures,
            "features": stats.feature_bytes,
            "audio": stats.bytes_audio,
        },
    });
    write_json_gz(
        &out_root.join("index.json"),
        &serde_json::to_vec_pretty(&index)?,
    )?;

    println!(
        "\n=== 完成（{:.2?}）：imgs={} trees={}（fail {}） strings={}（fail {}） sheets={}（fail {}） textures={}（fail {}） features={}（fail {}） audio={}（fail {}）",
        t0.elapsed(),
        stats.imgs,
        stats.trees,
        stats.tree_fail,
        stats.strings,
        stats.string_fail,
        stats.sheets,
        stats.sheet_fail,
        stats.textures,
        stats.texture_fail,
        stats.features_ok,
        stats.features_fail,
        stats.audio,
        stats.audio_fail
    );
    println!(
        "canvas: total={} resolved={}（{:.2}%）",
        stats.canvas_total,
        stats.canvas_resolved,
        stats.canvas_resolved as f64 / stats.canvas_total.max(1) as f64 * 100.0
    );
    Ok(())
}

// ── tree/ ────────────────────────────────────────────────────

fn export_trees(
    svc: &DataService,
    out_root: &Path,
    limit: Option<usize>,
    stats: &mut Stats,
) -> Result<(), Box<dyn std::error::Error>> {
    let tree_dir = out_root.join("tree");
    let imgs: Vec<&String> = svc
        .catalog
        .entries
        .iter()
        .filter(|(_, loc)| loc.class_id == objects::CLASS_MONO_BEHAVIOUR)
        .map(|(path, _)| path)
        .collect();
    stats.imgs = imgs.len();
    let imgs = match limit {
        Some(n) => &imgs[..n.min(imgs.len())],
        None => &imgs[..],
    };
    println!("tree: 處理 {} / {} 個 img", imgs.len(), stats.imgs);

    for (i, img_key) in imgs.iter().enumerate() {
        if i > 0 && i % 500 == 0 {
            println!(
                "[tree] {i}/{}（累計 ok={} fail={} canvas {}/{}）",
                imgs.len(),
                stats.trees,
                stats.tree_fail,
                stats.canvas_resolved,
                stats.canvas_total
            );
        }
        let result = export_one_tree(svc, img_key, &tree_dir, stats);
        match result {
            Ok(bytes) => {
                stats.trees += 1;
                stats.bytes_tree += bytes;
            }
            Err(e) => {
                if stats.tree_fail < 10 {
                    println!("!! [tree] {img_key}: {e}");
                }
                stats.tree_fail += 1;
            }
        }
    }
    println!(
        "[tree] 完成：ok={} fail={}，canvas {}/{}",
        stats.trees, stats.tree_fail, stats.canvas_resolved, stats.canvas_total
    );
    Ok(())
}

fn export_one_tree(
    svc: &DataService,
    img_key: &str,
    tree_dir: &Path,
    stats: &mut Stats,
) -> Result<u64, Box<dyn std::error::Error>> {
    let tree = svc.tree_for(img_key)?;

    // 同一 img 的 spritesheet（key == img 邏輯路徑）。
    let sheet_index = match svc.catalog.image.sheets.get(img_key) {
        Some(sheet) => match &sheet.index_loc {
            Some(index_loc) => {
                let payload = svc.read_entry(index_loc)?;
                let mono = objects::parse_mono_behaviour(&payload, true)?;
                let body = payload
                    .get(mono.header_len..)
                    .ok_or("SpriteSheet MonoBehaviour header 越界")?;
                Some(if sheet.index_is_asset {
                    SheetLookup::Asset(parse_sheet_asset(body)?)
                } else {
                    SheetLookup::Wzss(parse_wzss(body)?)
                })
            }
            None => None,
        },
        None => None,
    };

    let root = node_to_json(&tree, "", img_key, sheet_index.as_ref(), stats);
    let json = serde_json::to_vec(&root)?;
    let out_path = tree_dir.join(safe_rel_path(img_key)).with_extension("json");
    write_json_gz(&out_path, &json)
}

enum SheetLookup {
    Wzss(WzssIndex),
    Asset(SheetAsset),
}

impl SheetLookup {
    fn sprite_for_node(&self, inner: &str) -> Option<WzssSprite> {
        match self {
            Self::Wzss(index) => index
                .frame_index_for_node(inner)
                .and_then(|frame| index.sprite_for_frame(frame)),
            Self::Asset(index) => index.sprite_for_node(inner),
        }
    }
}

/// WzNode → 託管用 tree JSON（自訂形狀，不動 WzNode 本身的 Serialize）。
/// `inner` 是 img 內相對路徑（根為 ""），用來查 SpriteSheet 節點鍵。
fn node_to_json(
    node: &WzNode,
    inner: &str,
    sheet_key: &str,
    sheet_index: Option<&SheetLookup>,
    stats: &mut Stats,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "name": node.name,
        "tag": node.type_tag,
        "value": serde_json::to_value(&node.value).unwrap_or(serde_json::Value::Null),
    });
    let map = obj.as_object_mut().expect("剛建的 JSON 物件");

    if is_canvas_tag(node.type_tag) {
        stats.canvas_total += 1;
        let sprite = sheet_index.and_then(|index| index.sprite_for_node(inner));
        if let Some(sprite) = sprite {
            stats.canvas_resolved += 1;
            let (ox, oy) = canvas_origin(node);
            map.insert(
                "canvas".to_string(),
                serde_json::json!({
                    "sheet": sheet_key,
                    "page": sprite.page,
                    "x": sprite.rect.x,
                    "y": sprite.rect.y,
                    "w": sprite.rect.w,
                    "h": sprite.rect.h,
                    "ox": ox,
                    "oy": oy,
                }),
            );
        } else {
            map.insert("canvas".to_string(), serde_json::Value::Null);
        }
    }

    if !node.children.is_empty() {
        let children: Vec<serde_json::Value> = node
            .children
            .iter()
            .map(|child| {
                let child_inner = if inner.is_empty() {
                    child.name.clone()
                } else {
                    format!("{inner}/{}", child.name)
                };
                node_to_json(child, &child_inner, sheet_key, sheet_index, stats)
            })
            .collect();
        map.insert("children".to_string(), serde_json::Value::Array(children));
    }
    obj
}

/// canvas 的 origin 子節點（`Value::Vector`），無則 (0, 0)。
fn canvas_origin(node: &WzNode) -> (f32, f32) {
    match node.children.iter().find(|c| c.name == "origin") {
        Some(WzNode {
            value: Value::Vector(x, y),
            ..
        }) => (*x, *y),
        _ => (0.0, 0.0),
    }
}

// ── strings/ ─────────────────────────────────────────────────

fn export_strings(
    svc: &DataService,
    out_root: &Path,
    stats: &mut Stats,
) -> Result<(), Box<dyn std::error::Error>> {
    let strings_dir = out_root.join("strings");
    for (path, loc) in &svc.catalog.entries {
        if loc.class_id != objects::CLASS_TEXT_ASSET {
            continue;
        }
        match svc.get_node(path) {
            Ok(detail) => {
                let Some(text) = detail.text_asset else {
                    stats.string_fail += 1;
                    continue;
                };
                let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
                let out_path = strings_dir.join(safe_rel_path(path)).with_extension("json");
                stats.bytes_strings += write_json_gz(&out_path, text.as_bytes())?;
                stats.strings += 1;
            }
            Err(e) => {
                if stats.string_fail < 10 {
                    println!("!! [strings] {path}: {e}");
                }
                stats.string_fail += 1;
            }
        }
    }
    println!(
        "[strings] 完成：ok={} fail={}",
        stats.strings, stats.string_fail
    );
    Ok(())
}

// ── sheets/ ──────────────────────────────────────────────────

/// 按 bundle_index 快取 Bundle 控制代碼（DataService 的是 pub(crate)，自己開）。
struct BundleCache<'a> {
    svc: &'a DataService,
    handles: HashMap<u32, Bundle>,
}

impl<'a> BundleCache<'a> {
    fn new(svc: &'a DataService) -> Self {
        Self {
            svc,
            handles: HashMap::new(),
        }
    }

    fn get(&mut self, bundle_index: u32) -> Result<&Bundle, Box<dyn std::error::Error>> {
        Ok(self.handles.entry(bundle_index).or_insert_with_key(|i| {
            let meta = &self.svc.catalog.bundles[*i as usize];
            Bundle::open(&self.svc.catalog.root.join(format!("{}.bundle", meta.name)))
                .expect("開啟 bundle 失敗")
        }))
    }
}

fn export_sheets(
    svc: &DataService,
    out_root: &Path,
    limit: Option<usize>,
    stats: &mut Stats,
) -> Result<(), Box<dyn std::error::Error>> {
    let sheets_dir = out_root.join("sheets");
    let total_pages: usize = svc
        .catalog
        .image
        .sheets
        .values()
        .map(|s| s.atlas_pages.len())
        .sum();
    println!("[sheets] 共 {} 頁（limit={limit:?}）", total_pages);
    let mut bundles = BundleCache::new(svc);

    'outer: for (key, sheet) in &svc.catalog.image.sheets {
        for (page, loc) in &sheet.atlas_pages {
            if limit.is_some_and(|n| stats.sheets + stats.sheet_fail >= n) {
                break 'outer;
            }
            match export_texture_png(svc, &mut bundles, loc) {
                Ok((png, _name)) => {
                    let out_path = sheets_dir.join(safe_rel_path(key)).with_file_name(format!(
                        "{}_{}.png",
                        sanitize_segment(key.rsplit('/').next().expect("split 至少一段")),
                        page
                    ));
                    if let Some(parent) = out_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    stats.bytes_sheets += png.len() as u64;
                    std::fs::write(&out_path, png)?;
                    stats.sheets += 1;
                }
                Err(e) => {
                    if stats.sheet_fail < 10 {
                        println!("!! [sheets] {key}_{page}: {e}");
                    }
                    stats.sheet_fail += 1;
                }
            }
            if (stats.sheets + stats.sheet_fail).is_multiple_of(500) {
                println!(
                    "[sheets] 累計 ok={} fail={}",
                    stats.sheets, stats.sheet_fail
                );
            }
        }
    }
    println!(
        "[sheets] 完成：ok={} fail={}",
        stats.sheets, stats.sheet_fail
    );
    Ok(())
}

/// EntryLocation（Texture2D）→ PNG bytes（已翻正）。
fn export_texture_png(
    svc: &DataService,
    bundles: &mut BundleCache<'_>,
    loc: &EntryLocation,
) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
    let data = svc.read_entry(loc)?;
    let tex = objects::parse_texture2d(&data, true)?;
    let bundle = bundles.get(loc.bundle_index)?;
    let mut rgba = texture::fetch_pixels(bundle, loc.node_index as usize, &tex, &data)?;
    let width = tex.width as u32;
    let height = tex.height as u32;
    texture::flip_vertical(&mut rgba, width, height)?;
    let png = texture::to_png(width, height, &rgba)?;
    Ok((png, tex.name))
}

// ── textures/（不屬於任何 sheet atlas 頁的 Texture2D）────────

fn export_textures(
    svc: &DataService,
    out_root: &Path,
    stats: &mut Stats,
) -> Result<(), Box<dyn std::error::Error>> {
    // sheet atlas 頁的位置集合：掃到就跳過（以 sheets/ 命名為準，不重複寫）。
    let sheet_pages: HashSet<(u32, u32, u64)> = svc
        .catalog
        .image
        .sheets
        .values()
        .flat_map(|s| s.atlas_pages.iter())
        .map(|(_, loc)| (loc.bundle_index, loc.node_index, loc.abs_offset))
        .collect();

    let tex_dir = out_root.join("textures");
    let mut bundles = BundleCache::new(svc);
    let mut done = 0usize;
    for (bundle_index, meta) in svc.catalog.bundles.iter().enumerate() {
        let stem = &meta.name;
        if !(stem.starts_with("spriteset_")
            || stem.starts_with("spritesheet_")
            || stem.starts_with("atlas_"))
        {
            continue;
        }
        let bundle = bundles.get(bundle_index as u32)?;
        for ni in 0..bundle.nodes.len() {
            if !bundle.nodes[ni].is_serialized_file() {
                continue;
            }
            let head48 = bundle.read_node_range(ni, 0, HEADER_SIZE)?;
            let meta_len = metadata_len(&head48)?;
            let head = bundle.read_node_range(ni, 0, meta_len)?;
            let sf = SerializedFile::parse(&head)?;
            for o in sf.objects_by_class(objects::CLASS_TEXTURE2D) {
                let abs_offset = sf.data_offset + o.byte_start as u64;
                if sheet_pages.contains(&(bundle_index as u32, ni as u32, abs_offset)) {
                    continue;
                }
                let result = (|| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
                    let data = bundle.read_node_range(ni, abs_offset, o.byte_size as usize)?;
                    let tex = objects::parse_texture2d(&data, &sf)?;
                    let mut rgba = texture::fetch_pixels(bundle, ni, &tex, &data)?;
                    let (w, h) = (tex.width as u32, tex.height as u32);
                    texture::flip_vertical(&mut rgba, w, h)?;
                    let png = texture::to_png(w, h, &rgba)?;
                    let out_path = tex_dir.join(sanitize_segment(stem)).join(format!(
                        "{}_{}.png",
                        sanitize_file_name(&tex.name),
                        o.path_id
                    ));
                    if let Some(parent) = out_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&out_path, &png)?;
                    Ok(png)
                })();
                match result {
                    Ok(png) => {
                        stats.textures += 1;
                        stats.bytes_textures += png.len() as u64;
                    }
                    Err(e) => {
                        if stats.texture_fail < 10 {
                            println!("!! [textures] [{stem}] pid {}: {e}", o.path_id);
                        }
                        stats.texture_fail += 1;
                    }
                }
                done += 1;
                if done.is_multiple_of(1000) {
                    println!(
                        "[textures] 掃描 {done}（累計 ok={} fail={}）",
                        stats.textures, stats.texture_fail
                    );
                }
            }
        }
        println!(
            "[textures] [{stem}] 完成（累計 ok={} fail={}）",
            stats.textures, stats.texture_fail
        );
    }
    Ok(())
}

// ── features/（聚合資料）──────────────────────────────────────

/// features 階段：直接呼叫 DataService / avatar 的 pub 聚合函式，
/// 各檔獨立失敗（印警告、fail 計數），不影響其他檔與整個 pipeline。
fn export_features(
    svc: &DataService,
    out_root: &Path,
    stats: &mut Stats,
) -> Result<(), Box<dyn std::error::Error>> {
    let features_dir = out_root.join("features");
    std::fs::create_dir_all(&features_dir)?;

    feature_step(&features_dir, "monster_book.json", stats, || {
        let book = svc.get_monster_book()?;
        Ok((serde_json::to_vec(&*book)?, book.len()))
    });
    feature_step(&features_dir, "skill_book.json", stats, || {
        let book = svc.get_skill_book()?;
        Ok((serde_json::to_vec(&*book)?, book.len()))
    });
    feature_step(&features_dir, "quest_book.json", stats, || {
        let book = svc.get_quest_book()?;
        Ok((serde_json::to_vec(&*book)?, book.len()))
    });
    feature_step(&features_dir, "map_bgm.json", stats, || {
        let index = svc.get_map_bgm_index()?;
        Ok((serde_json::to_vec(&*index)?, index.len()))
    });
    feature_step(&features_dir, "commodity.json", stats, || {
        let items = svc.get_commodity_items()?;
        Ok((serde_json::to_vec(&items)?, items.len()))
    });
    feature_step(&features_dir, "avatar_equipment.json", stats, || {
        let items = msclassic::avatar::avatar_equipment(svc)?;
        Ok((serde_json::to_vec(&items)?, items.len()))
    });
    feature_step(&features_dir, "avatar_meta.json", stats, || {
        let meta = msclassic::avatar::avatar_meta(svc)?;
        Ok((serde_json::to_vec(&meta)?, meta.actions.len()))
    });
    feature_step(&features_dir, "pet_list.json", stats, || {
        let pets = msclassic::pet::pet_list(svc)?;
        Ok((serde_json::to_vec(&pets)?, pets.len()))
    });
    feature_step(&features_dir, "pet_equip_list.json", stats, || {
        let items = msclassic::pet::pet_equip_list(svc)?;
        Ok((serde_json::to_vec(&items)?, items.len()))
    });
    feature_step(&features_dir, "action_delays.json", stats, || {
        // 語意照抄 skill.rs 的 action_delays：`Character/00002000` 根的非 info
        // 子節點各為一個動作，其幀子節點的 `delay` 取絕對值加總；幀缺 delay
        // 或值非數字時該幀無貢獻。
        let tree = svc.tree_for("Character/00002000")?;
        let delays: serde_json::Map<String, serde_json::Value> = tree
            .children
            .iter()
            .filter(|action| action.name != "info")
            .map(|action| {
                let total: i64 = action
                    .children
                    .iter()
                    .filter_map(|frame| frame.find("delay").and_then(|n| node_as_i64(&n.value)))
                    .map(i64::abs)
                    .sum();
                (action.name.clone(), serde_json::json!(total))
            })
            .collect();
        let count = delays.len();
        Ok((serde_json::to_vec(&delays)?, count))
    });

    println!(
        "[features] 完成：ok={} fail={}",
        stats.features_ok, stats.features_fail
    );
    Ok(())
}

/// 跑一個 features 檔的產出（run 回 JSON bytes 與清單筆數），寫檔並記帳；
/// 任何一步失敗只印警告、累計 fail。
fn feature_step(
    dir: &Path,
    file: &str,
    stats: &mut Stats,
    run: impl FnOnce() -> Result<(Vec<u8>, usize), Box<dyn std::error::Error>>,
) {
    let result = run().and_then(|(json, count)| {
        let bytes = write_json_gz(&dir.join(file), &json)?;
        Ok((bytes, count))
    });
    match result {
        Ok((bytes, count)) => {
            let key = file.trim_end_matches(".json");
            stats
                .feature_counts
                .insert(key.to_string(), serde_json::json!(count));
            stats
                .feature_bytes
                .insert(key.to_string(), serde_json::json!(bytes));
            stats.features_ok += 1;
            println!("[features] {file}.gz: {count} 筆（{bytes} bytes）");
        }
        Err(e) => {
            println!("!! [features] {file}: {e}");
            stats.features_fail += 1;
        }
    }
}

/// 數值節點 → i64（與 skill.rs 的 value_as_i64 同規則；非數值回 None）。
fn node_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::U8(v) => Some(i64::from(*v)),
        Value::U16(v) => Some(i64::from(*v)),
        Value::Int(v) => Some(i64::from(*v)),
        Value::Long(v) => Some(*v),
        Value::Int2(a, _) => Some(i64::from(*a)),
        Value::Double(v) => Some(*v as i64),
        _ => None,
    }
}

// ── audio/（AudioClip → Ogg Vorbis）─────────────────────────

fn export_audio(
    svc: &DataService,
    out_root: &Path,
    stats: &mut Stats,
) -> Result<(), Box<dyn std::error::Error>> {
    let audio_dir = out_root.join("audio");
    let clips: Vec<&String> = svc
        .catalog
        .entries
        .iter()
        .filter(|(_, loc)| loc.class_id == objects::CLASS_AUDIO_CLIP)
        .map(|(path, _)| path)
        .collect();
    println!("[audio] 共 {} 個 AudioClip", clips.len());

    for (i, path) in clips.iter().enumerate() {
        if i > 0 && i % 500 == 0 {
            println!(
                "[audio] {i}/{}（累計 ok={} fail={}）",
                clips.len(),
                stats.audio,
                stats.audio_fail
            );
        }
        let result = (|| -> Result<u64, Box<dyn std::error::Error>> {
            let ogg = svc.get_audio(path)?;
            let out_path = audio_dir.join(safe_rel_path(path)).with_extension("ogg");
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let len = ogg.len() as u64;
            std::fs::write(out_path, ogg)?;
            Ok(len)
        })();
        match result {
            Ok(bytes) => {
                stats.audio += 1;
                stats.bytes_audio += bytes;
            }
            Err(e) => {
                if stats.audio_fail < 10 {
                    println!("!! [audio] {path}: {e}");
                }
                stats.audio_fail += 1;
            }
        }
    }
    println!("[audio] 完成：ok={} fail={}", stats.audio, stats.audio_fail);
    Ok(())
}

// ── CLI / 工具 ───────────────────────────────────────────────

fn parse_args() -> Result<Args, Box<dyn std::error::Error>> {
    let mut positional = Vec::new();
    let mut version = None;
    let mut limit_imgs = None;
    let mut limit_sheets = None;
    let mut skip_trees = false;
    let mut skip_strings = false;
    let mut skip_sheets = false;
    let mut skip_textures = false;
    let mut skip_features = false;
    let mut skip_audio = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--version" => version = Some(it.next().ok_or("--version 需要一個值")?),
            "--limit-imgs" => {
                limit_imgs = Some(it.next().ok_or("--limit-imgs 需要一個值")?.parse()?)
            }
            "--limit-sheets" => {
                limit_sheets = Some(it.next().ok_or("--limit-sheets 需要一個值")?.parse()?)
            }
            "--skip-trees" => skip_trees = true,
            "--skip-strings" => skip_strings = true,
            "--skip-sheets" => skip_sheets = true,
            "--skip-textures" => skip_textures = true,
            "--skip-features" => skip_features = true,
            "--skip-audio" => skip_audio = true,
            other if other.starts_with("--") => return Err(format!("未知參數: {other}").into()),
            _ => positional.push(arg),
        }
    }
    if positional.len() != 2 {
        return Err("用法: hosted_export <bundle_dir> <out_dir> [--version V] [--limit-imgs N] [--limit-sheets N] [--skip-trees] [--skip-strings] [--skip-sheets] [--skip-textures] [--skip-features] [--skip-audio]".into());
    }
    let (year, month, day) = today();
    Ok(Args {
        bundle_dir: PathBuf::from(&positional[0]),
        out_dir: PathBuf::from(&positional[1]),
        version: version.unwrap_or_else(|| format!("v{year:04}{month:02}{day:02}")),
        generated_at: format!("{year:04}-{month:02}-{day:02}"),
        limit_imgs,
        limit_sheets,
        skip_trees,
        skip_strings,
        skip_sheets,
        skip_textures,
        skip_features,
        skip_audio,
    })
}

/// 當天日期（UTC；pipeline 用，精度夠了）。
fn today() -> (i64, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_from_days((secs / 86_400) as i64)
}

/// days since 1970-01-01 → (year, month, day)（Howard Hinnant 演算法）。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// 寫 gzip 壓縮 JSON：壓縮後寫到 `<json_path>.gz`，並刪除同路徑的舊
/// 未壓縮 `.json`（避免新舊產物並存）。父目錄自動建立；回傳壓縮後位元組數。
fn write_json_gz(json_path: &Path, data: &[u8]) -> Result<u64, Box<dyn std::error::Error>> {
    if let Some(parent) = json_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data)?;
    let gz = encoder.finish()?;
    let mut gz_name = json_path.as_os_str().to_owned();
    gz_name.push(".gz");
    let len = gz.len() as u64;
    std::fs::write(gz_name, gz)?;
    match std::fs::remove_file(json_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(len)
}

/// 邏輯路徑 → 安全的相對路徑（逐段消毒，防 `..`、絕對路徑與 Windows 保留字）。
fn safe_rel_path(logical: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for seg in logical.split(['/', '\\']) {
        out.push(sanitize_segment(seg));
    }
    out
}

fn sanitize_segment(seg: &str) -> String {
    let cleaned = sanitize_file_name(seg);
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return "_".to_string();
    }
    let stem = cleaned.split('.').next().unwrap_or("");
    if matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return format!("_{cleaned}");
    }
    cleaned
}

/// 檔名消毒：`<>:"/\|?*` 與控制字元換成 `_`。
fn sanitize_file_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

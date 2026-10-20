//! 子樹匯出（Phase 5）：選取節點 → 目錄樹形式的 JSON + PNG。
//!
//! 輸出結構（以 `TamingMob/0004` 為例）：
//! ```text
//! <out>/TamingMob/0004.json          巢狀 JSON，整棵子樹（可 grep、可 diff）
//! <out>/TamingMob/0004/walk1/0.png   canvas 節點的圖片（JSON 內以 "$png" 參照）
//! ```
//!
//! JSON 形狀：
//! - 葉節點：純量（number / string），vector/Float2 是 `[x, y]`
//! - 有子節點：物件 `{ 子名: ... }`；canvas 節點額外帶 `"$type": "canvas"` 與 `"$png"`
//! - 兄弟同名（理論上不該發生）時自動加 `#2`、`#3` 後綴避免覆蓋

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map};

use crate::catalog::{split_img_prefix, CatalogError, DataService, Result};
use crate::unity::objects;
use crate::wzjs::{Value, WzNode};

/// 匯出寫檔的集中點（桌面行為不變：直接落碟）。
#[cfg(not(target_arch = "wasm32"))]
fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(CatalogError::Io)
}

// wasm 單檔匯出的記憶體攔截：`Some` 時 write_file 收進這裡，不經 JS。
// （wasm 單執行緒，匯出全程同步，不會與 JS 收集器路徑交錯。）
#[cfg(target_arch = "wasm32")]
type ExportCapture = Option<Vec<(String, Vec<u8>)>>;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static EXPORT_CAPTURE: std::cell::RefCell<ExportCapture> =
        const { std::cell::RefCell::new(None) };
}

/// wasm 的寫檔：單檔匯出攔截到記憶體；其餘逐檔經 JS shim 的 `exportFile`
/// 丟給 JS 收集器（路徑統一用 `/` 分隔，JS 端打包 zip 或觸發下載）。
#[cfg(target_arch = "wasm32")]
fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let path = path.to_string_lossy().replace('\\', "/");
    let captured = EXPORT_CAPTURE.with(|capture| {
        if let Some(buffer) = capture.borrow_mut().as_mut() {
            buffer.push((path.clone(), bytes.to_vec()));
            true
        } else {
            false
        }
    });
    if captured {
        return Ok(());
    }
    if crate::wasm_shims::export_file_to_js(&path, bytes) {
        Ok(())
    } else {
        Err(CatalogError::CacheInvalid(format!(
            "JS 端拒絕匯出檔案: {path}"
        )))
    }
}

/// 確保檔案的父目錄存在（桌面實體落碟前建立）。
#[cfg(not(target_arch = "wasm32"))]
fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(CatalogError::Io)?;
    }
    Ok(())
}

/// wasm 無實體目錄，路徑只是給 JS 的字串，不需建目錄。
#[cfg(target_arch = "wasm32")]
fn ensure_parent_dir(_path: &Path) -> Result<()> {
    Ok(())
}

/// PNG 去重的讀回比對；讀不到就當不存在（多寫一份，不影響正確性）。
#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// wasm 沒有已匯出檔案可讀回，一律視為無重複（Step 2 接 ExportSink 後再議）。
#[cfg(target_arch = "wasm32")]
fn read_file(_path: &Path) -> Option<Vec<u8>> {
    None
}

#[derive(Debug, Serialize)]
pub struct ExportStats {
    pub out_dir: String,
    pub json_files: usize,
    pub png_files: usize,
    pub nodes: usize,
}

#[derive(Debug, Serialize)]
pub struct FileExportResult {
    pub out_path: String,
    pub nodes: usize,
}

impl DataService {
    /// 匯出 path 指向的子樹（img 根或樹內節點皆可）。TextAsset 直接輸出明文 JSON。
    /// 回傳輸出位置與統計。
    pub fn export_subtree(&self, path: &str, out_root: &Path) -> Result<ExportStats> {
        let (img_key, inner) = split_img_prefix(&self.catalog, path)
            .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?;
        let loc = self
            .catalog
            .entries
            .get(&img_key)
            .ok_or_else(|| CatalogError::PathNotFound(img_key.clone()))?
            .clone();

        // 先完整解析 domain path，再進行任何檔案操作。
        if loc.class_id == objects::CLASS_TEXT_ASSET {
            let detail = self.get_node(path)?;
            let text = detail.text_asset.unwrap_or_default();
            let json_path = export_json_path(out_root, path)?;
            ensure_parent_dir(&json_path)?;
            write_file(&json_path, text.as_bytes())?;
            return Ok(ExportStats {
                out_dir: out_root.display().to_string(),
                json_files: 1,
                png_files: 0,
                nodes: 1,
            });
        }

        let root = self.tree_for(&img_key)?;
        let node: &WzNode = if inner.is_empty() {
            root.as_ref()
        } else {
            root.find(&inner)
                .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?
        };

        let safe_path = safe_relative_path(path)?;
        let json_path = append_extension(out_root.join(&safe_path), "json")?;
        ensure_parent_dir(&json_path)?;

        // PNG 目錄：<out>/<path>/ 底下的相對路徑與 JSON 的 "$png" 一致
        let png_base = out_root.join(safe_path);
        let mut ctx = ExportCtx {
            svc_path: path.to_string(),
            png_base,
            png_files: 0,
            nodes: 0,
            source_stack: Vec::new(),
            file_stack: Vec::new(),
            seen_png: std::collections::HashMap::new(),
        };
        let value = self.node_to_json(node, &mut ctx)?;
        let pretty = serde_json::to_string_pretty(&value)
            .map_err(|e| CatalogError::PathNotFound(format!("JSON 序列化失敗: {e}")))?;
        write_file(&json_path, pretty.as_bytes())?;

        Ok(ExportStats {
            out_dir: out_root.display().to_string(),
            json_files: 1,
            png_files: ctx.png_files,
            nodes: ctx.nodes,
        })
    }

    /// 將目前選取的節點匯出為單一 JSON、PNG 或 XML 檔案。
    ///
    /// JSON 與 XML 只包含資料結構；canvas 只保留型別標記，不嵌入或額外輸出圖片。
    /// PNG 則只輸出目前節點解析到的圖片。
    pub fn export_file(
        &self,
        path: &str,
        output_path: &Path,
        format: &str,
    ) -> Result<FileExportResult> {
        ensure_parent_dir(output_path)?;

        if format.eq_ignore_ascii_case("png") {
            let detail = self.get_node(path)?;
            if !crate::wzjs::node::is_image_capable_tag(detail.type_tag) {
                return Err(CatalogError::PathNotFound(format!(
                    "{path} 不是可匯出圖片的節點（需為 Canvas 或 UOL）"
                )));
            }
            let png = self
                .get_image(path)?
                .ok_or_else(|| CatalogError::PathNotFound(format!("{path} 沒有可匯出的圖片")))?;
            write_file(output_path, &png)?;
            return Ok(FileExportResult {
                out_path: output_path.display().to_string(),
                nodes: 1,
            });
        }

        let (img_key, inner) = split_img_prefix(&self.catalog, path)
            .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?;
        let loc = self
            .catalog
            .entries
            .get(&img_key)
            .ok_or_else(|| CatalogError::PathNotFound(img_key.clone()))?;

        if loc.class_id == objects::CLASS_TEXT_ASSET {
            let detail = self.get_node(path)?;
            let text = detail.text_asset.unwrap_or_default();
            let output = if format.eq_ignore_ascii_case("json") {
                serde_json::to_string_pretty(&text).map_err(|error| {
                    CatalogError::PathNotFound(format!("JSON 序列化失敗: {error}"))
                })?
            } else if format.eq_ignore_ascii_case("xml") {
                format!(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                     <text-asset path=\"{}\">{}</text-asset>\n",
                    escape_xml_attribute(path),
                    escape_xml_text(&text)
                )
            } else {
                return Err(CatalogError::PathNotFound(format!(
                    "不支援的匯出格式: {format}"
                )));
            };
            write_file(output_path, output.as_bytes())?;
            return Ok(FileExportResult {
                out_path: output_path.display().to_string(),
                nodes: 1,
            });
        }

        let root = self.tree_for(&img_key)?;
        let node = if inner.is_empty() {
            root.as_ref()
        } else {
            root.find(&inner)
                .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?
        };
        let mut nodes = 0;
        let output = if format.eq_ignore_ascii_case("json") {
            let value = node_to_data_json(node, &mut nodes);
            serde_json::to_string_pretty(&value)
                .map_err(|error| CatalogError::PathNotFound(format!("JSON 序列化失敗: {error}")))?
        } else if format.eq_ignore_ascii_case("xml") {
            export_xml(path, node, &mut nodes)
        } else {
            return Err(CatalogError::PathNotFound(format!(
                "不支援的匯出格式: {format}"
            )));
        };
        write_file(output_path, output.as_bytes())?;
        Ok(FileExportResult {
            out_path: output_path.display().to_string(),
            nodes,
        })
    }

    /// wasm 單檔匯出：內容攔截到記憶體回傳 `(檔名, bytes)`，不經 JS 收集器。
    /// 供 web-wasm 的 `export_file` 組成下載回應；`export_subtree` 仍走 JS 收集器。
    #[cfg(target_arch = "wasm32")]
    pub fn export_file_to_memory(&self, path: &str, format: &str) -> Result<(String, Vec<u8>)> {
        let stem = path.rsplit('/').next().unwrap_or("export");
        let filename = format!("{stem}.{}", format.to_ascii_lowercase());
        EXPORT_CAPTURE.with(|capture| *capture.borrow_mut() = Some(Vec::new()));
        let result = self.export_file(path, Path::new(&filename), format);
        let captured = EXPORT_CAPTURE.with(|capture| capture.borrow_mut().take());
        result?;
        let mut files = captured.unwrap_or_default();
        if files.len() != 1 {
            return Err(CatalogError::CacheInvalid(format!(
                "單檔匯出預期產生 1 個檔案，實際 {} 個",
                files.len()
            )));
        }
        let (_, bytes) = files.remove(0);
        Ok((filename, bytes))
    }

    /// 節點 → 巢狀 JSON；canvas 順便輸出 PNG。
    fn node_to_json(&self, node: &WzNode, ctx: &mut ExportCtx) -> Result<serde_json::Value> {
        ctx.nodes += 1;
        let is_canvas = matches!(node.value, Value::Canvas);

        // 葉節點：純量直接回傳
        if node.children.is_empty() && !is_canvas {
            return Ok(leaf_json(&node.value));
        }

        let mut obj: Map<String, serde_json::Value> = Map::new();
        if is_canvas {
            obj.insert("$type".into(), json!("canvas"));
            // 匯出圖片；失敗（查無對應圖）不擋匯出，JSON 標註即可
            let source_rel = ctx.source_rel_path();
            let file_rel = ctx.file_rel_path();
            let full_path = if source_rel.is_empty() {
                ctx.svc_path.clone()
            } else {
                format!("{}/{}", ctx.svc_path, source_rel)
            };
            match self.get_image(&full_path) {
                Ok(Some(png)) => {
                    // 去重：frame 映射未知時整頁輸出會產生大量相同 PNG，
                    // 相同內容只寫一次，後續節點 $png 指向同一檔。
                    let hash = fnv1a(&png);
                    if let Some(existing) = ctx.find_identical_png(hash, &png) {
                        obj.insert("$png".into(), json!(existing));
                    } else {
                        let name = if file_rel.is_empty() {
                            "_root".to_string()
                        } else {
                            file_rel
                        };
                        let relative_file = format!("{name}.png");
                        let file = ctx.png_base.join(Path::new(&relative_file));
                        ensure_parent_dir(&file)?;
                        write_file(&file, &png)?;
                        ctx.png_files += 1;
                        ctx.seen_png
                            .entry(hash)
                            .or_default()
                            .push(relative_file.clone());
                        obj.insert("$png".into(), json!(relative_file));
                    }
                }
                _ => {
                    obj.insert("$png".into(), serde_json::Value::Null);
                }
            }
        }
        // 葉的純量值（有子節點的節點理論上是 SubTree/Canvas，但保險起見帶上）
        if !matches!(node.value, Value::SubTree | Value::Canvas) {
            obj.insert("$value".into(), leaf_json(&node.value));
        }

        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for child in &node.children {
            let count = seen.entry(child.name.clone()).or_insert(0);
            *count += 1;
            let key = if *count == 1 {
                child.name.clone()
            } else {
                format!("{}#{}", child.name, count)
            };
            ctx.source_stack.push(child.name.clone());
            ctx.file_stack.push(safe_segment(&key));
            let v = self.node_to_json(child, ctx)?;
            ctx.source_stack.pop();
            ctx.file_stack.pop();
            obj.insert(key, v);
        }
        Ok(serde_json::Value::Object(obj))
    }
}

/// 匯出過程的暫存：目前走到的相對路徑（給 PNG 命名與 get_image 用）。
struct ExportCtx {
    svc_path: String,
    png_base: PathBuf,
    png_files: usize,
    nodes: usize,
    source_stack: Vec<String>,
    file_stack: Vec<String>,
    /// 已輸出的 PNG 內容雜湊 → 檔名（去重用）。
    seen_png: std::collections::HashMap<u64, Vec<String>>,
}

/// FNV-1a 64-bit（PNG 內容去重，無外部依賴）。
fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl ExportCtx {
    fn source_rel_path(&self) -> String {
        self.source_stack.join("/")
    }

    fn file_rel_path(&self) -> String {
        self.file_stack.join("/")
    }

    fn find_identical_png(&self, hash: u64, png: &[u8]) -> Option<String> {
        self.seen_png.get(&hash)?.iter().find_map(|relative| {
            let existing = read_file(&self.png_base.join(Path::new(relative)))?;
            (existing == png).then(|| relative.clone())
        })
    }
}

fn export_json_path(out_root: &Path, logical_path: &str) -> Result<PathBuf> {
    append_extension(out_root.join(safe_relative_path(logical_path)?), "json")
}

fn append_extension(mut path: PathBuf, extension: &str) -> Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CatalogError::PathNotFound("匯出路徑缺少檔名".into()))?;
    path.set_file_name(format!("{name}.{extension}"));
    Ok(path)
}

fn safe_relative_path(path: &str) -> Result<PathBuf> {
    if path.is_empty() {
        return Err(CatalogError::PathNotFound("匯出路徑不可為空".into()));
    }
    let mut output = PathBuf::new();
    for segment in path.split('/') {
        if segment.is_empty() {
            return Err(CatalogError::PathNotFound(format!(
                "匯出路徑含空白段落: {path:?}"
            )));
        }
        output.push(safe_segment(segment));
    }
    Ok(output)
}

/// 將任意資產名稱編成單一、安全且可逆向辨識的 Windows 檔名段落。
fn safe_segment(segment: &str) -> String {
    let mut output = String::new();
    for byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.') {
            output.push(char::from(*byte));
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if output == "."
        || output == ".."
        || reserved
            .iter()
            .any(|name| output.eq_ignore_ascii_case(name))
    {
        output.bytes().map(|byte| format!("%{byte:02X}")).collect()
    } else {
        output
    }
}

/// 葉值 → JSON 純量（vector/Float2 攤成 [x, y]）。
fn leaf_json(v: &Value) -> serde_json::Value {
    match v {
        Value::SubTree => serde_json::Value::Null,
        Value::Canvas => json!({"$type": "canvas"}),
        Value::U8(n) => json!(n),
        Value::U16(n) => json!(n),
        Value::Int(n) => json!(n),
        Value::Long(n) => json!(n),
        Value::Double(f) => json!(f),
        Value::Float2(x, y) | Value::Vector(x, y) => json!([x, y]),
        Value::Int2(a, b) => json!([a, b]),
        Value::Str(s) | Value::Uol(s) => json!(s),
    }
}

/// 單檔 JSON 匯出使用的資料樹。Canvas 不輸出圖片，只保留 `$type`。
fn node_to_data_json(node: &WzNode, nodes: &mut usize) -> serde_json::Value {
    *nodes += 1;
    let is_canvas = matches!(node.value, Value::Canvas);
    if node.children.is_empty() && !is_canvas {
        return leaf_json(&node.value);
    }

    let mut output = Map::new();
    if is_canvas {
        output.insert("$type".into(), json!("canvas"));
    } else if !matches!(node.value, Value::SubTree) {
        output.insert("$value".into(), leaf_json(&node.value));
    }

    let mut seen = std::collections::HashMap::<String, usize>::new();
    for child in &node.children {
        let count = seen.entry(child.name.clone()).or_insert(0);
        *count += 1;
        let name = if *count == 1 {
            child.name.clone()
        } else {
            format!("{}#{}", child.name, count)
        };
        output.insert(name, node_to_data_json(child, nodes));
    }
    serde_json::Value::Object(output)
}

fn export_xml(path: &str, node: &WzNode, nodes: &mut usize) -> String {
    use std::fmt::Write as _;

    let mut output = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    let _ = writeln!(
        output,
        "<msclassic-export path=\"{}\">",
        escape_xml_attribute(path)
    );
    write_xml_node(node, 1, nodes, &mut output);
    output.push_str("</msclassic-export>\n");
    output
}

fn write_xml_node(node: &WzNode, depth: usize, nodes: &mut usize, output: &mut String) {
    use std::fmt::Write as _;

    *nodes += 1;
    let indent = "  ".repeat(depth);
    let name = escape_xml_attribute(&node.name);
    let type_name = value_type_name(&node.value);
    let has_value = !matches!(node.value, Value::SubTree | Value::Canvas);

    if node.children.is_empty() && !has_value {
        let _ = writeln!(
            output,
            "{indent}<node name=\"{name}\" type=\"{type_name}\" />"
        );
        return;
    }

    let _ = writeln!(
        output,
        "{indent}<node name=\"{name}\" type=\"{type_name}\">"
    );
    if has_value {
        let value = leaf_json(&node.value);
        let text = match value {
            serde_json::Value::String(value) => value,
            other => other.to_string(),
        };
        let _ = writeln!(
            output,
            "{indent}  <value>{}</value>",
            escape_xml_text(&text)
        );
    }
    for child in &node.children {
        write_xml_node(child, depth + 1, nodes, output);
    }
    let _ = writeln!(output, "{indent}</node>");
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::SubTree => "subtree",
        Value::Canvas => "canvas",
        Value::U8(_) => "u8",
        Value::U16(_) => "u16",
        Value::Int(_) => "int",
        Value::Long(_) => "long",
        Value::Float2(_, _) => "float2",
        Value::Int2(_, _) => "int2",
        Value::Double(_) => "double",
        Value::Vector(_, _) => "vector",
        Value::Str(_) => "string",
        Value::Uol(_) => "uol",
    }
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_xml_attribute(value: &str) -> String {
    escape_xml_text(value)
        .replace('"', "&quot;")
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaf_json_scalars() {
        assert_eq!(leaf_json(&Value::Int(-30)), json!(-30));
        assert_eq!(leaf_json(&Value::Str("Pn".into())), json!("Pn"));
        assert_eq!(leaf_json(&Value::Vector(1.5, -2.0)), json!([1.5, -2.0]));
        assert_eq!(leaf_json(&Value::Double(0.3)), json!(0.3));
    }

    #[test]
    fn safe_export_path_cannot_escape_root() {
        let safe = safe_relative_path("Character/../../CON/a\\b:中文").unwrap();
        let components: Vec<_> = safe
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(components.len(), 5);
        assert_eq!(components[1], "%2E%2E");
        assert_eq!(components[2], "%2E%2E");
        assert_eq!(components[3], "%43%4F%4E");
        assert!(!components[4].contains('\\'));
        assert!(!components[4].contains(':'));
    }

    #[test]
    fn data_json_and_xml_do_not_embed_images() {
        let node = WzNode {
            name: "icon<&".into(),
            value: Value::Canvas,
            path: None,
            children: vec![WzNode {
                name: "origin".into(),
                value: Value::Vector(1.0, 2.0),
                path: None,
                children: vec![],
                type_tag: 14,
            }],
            type_tag: 18,
        };
        let mut json_nodes = 0;
        let json = node_to_data_json(&node, &mut json_nodes);
        assert_eq!(json_nodes, 2);
        assert_eq!(json["$type"], "canvas");
        assert!(json.get("$png").is_none());

        let mut xml_nodes = 0;
        let xml = export_xml("Character/<icon>", &node, &mut xml_nodes);
        assert_eq!(xml_nodes, 2);
        assert!(xml.contains("Character/&lt;icon&gt;"));
        assert!(xml.contains("name=\"icon&lt;&amp;\" type=\"canvas\""));
        assert!(!xml.contains("$png"));
    }
}
